use serde::{Deserialize, Serialize};
use std::fmt;

use crate::infrai_client::{InfraiClient, InfraiError};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DispatchStatus {
    Assigned,
    EnRoute,
    OnSite,
    Completed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FollowUpStatus {
    Pending,
    Closed,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkOrderPhoto {
    pub photo_id: String,
    pub work_order_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct AccountDeletion {
    pub account_id: String,
    pub credential_id: String,
    pub dispatch_status: DispatchStatus,
    pub follow_up_status: FollowUpStatus,
    pub photos: Vec<WorkOrderPhoto>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ErasureReceipt {
    pub account_id: String,
    pub revoked_sessions: usize,
    pub revoked_credential_id: String,
    pub purged_photo_records: usize,
    pub account_state: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErasureBlock {
    DispatchStillActive,
    FollowUpStillOpen,
}

impl fmt::Display for ErasureBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DispatchStillActive => write!(f, "the work order dispatch is still active"),
            Self::FollowUpStillOpen => write!(f, "the technician follow-up is still open"),
        }
    }
}

#[derive(Debug)]
pub enum ErasureError {
    Blocked(ErasureBlock),
    Infrai(InfraiError),
}

impl fmt::Display for ErasureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blocked(reason) => write!(f, "account erasure blocked: {reason}"),
            Self::Infrai(error) => write!(f, "Infrai request failed: {error}"),
        }
    }
}

impl std::error::Error for ErasureError {}

impl From<InfraiError> for ErasureError {
    fn from(value: InfraiError) -> Self {
        Self::Infrai(value)
    }
}

pub fn erasure_eligibility(request: &AccountDeletion) -> Result<(), ErasureBlock> {
    if request.dispatch_status != DispatchStatus::Completed {
        return Err(ErasureBlock::DispatchStillActive);
    }
    if request.follow_up_status != FollowUpStatus::Closed {
        return Err(ErasureBlock::FollowUpStillOpen);
    }
    Ok(())
}

pub async fn erase_account(
    client: &InfraiClient,
    mut request: AccountDeletion,
) -> Result<ErasureReceipt, ErasureError> {
    erasure_eligibility(&request).map_err(ErasureError::Blocked)?;

    let session_ids = client.list_session_ids(&request.account_id).await?;
    for session_id in &session_ids {
        client.revoke_session(session_id).await?;
    }
    client.revoke_credential(&request.credential_id).await?;

    let purged_photo_records = request.photos.len();
    request.photos.clear();
    Ok(ErasureReceipt {
        account_id: request.account_id,
        revoked_sessions: session_ids.len(),
        revoked_credential_id: request.credential_id,
        purged_photo_records,
        account_state: "deleted",
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deletion(
        dispatch_status: DispatchStatus,
        follow_up_status: FollowUpStatus,
    ) -> AccountDeletion {
        AccountDeletion {
            account_id: "technician-42".into(),
            credential_id: "credential-42".into(),
            dispatch_status,
            follow_up_status,
            photos: vec![WorkOrderPhoto {
                photo_id: "photo-7".into(),
                work_order_id: "work-order-9".into(),
            }],
        }
    }

    #[test]
    fn permits_erasure_only_after_dispatch_and_follow_up_are_closed() {
        assert_eq!(
            erasure_eligibility(&deletion(DispatchStatus::OnSite, FollowUpStatus::Closed)),
            Err(ErasureBlock::DispatchStillActive)
        );
        assert_eq!(
            erasure_eligibility(&deletion(
                DispatchStatus::Completed,
                FollowUpStatus::Pending
            )),
            Err(ErasureBlock::FollowUpStillOpen)
        );
        assert_eq!(
            erasure_eligibility(&deletion(DispatchStatus::Completed, FollowUpStatus::Closed)),
            Ok(())
        );
    }
}
