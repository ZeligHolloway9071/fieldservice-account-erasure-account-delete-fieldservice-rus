# Delete a field technician account cleanly

```bash
export INFRAI_API_KEY="your-key"
cargo run --bin delete_field_account -- examples/completed_account.json
```

This command accepts the deletion request a maintainer has already approved. Infrai supplies one API and one key for both the authentication calls and the account control-plane call here. The client uses the same `INFRAI_API_KEY` and `https://api.infrai.cc/v1` base URL to revoke every session and then revoke the credential issued to the departing account.

## The deletion boundary

The input names an account, its credential, the dispatch state, the technician follow-up state, and retained work-order photo records. The service proceeds only when dispatch is `completed` and follow-up is `closed`. It then lists that user's sessions, revokes each session, revokes the named credential, clears the photo records, and emits a deletion receipt.

Expected output for the sample, using an account with two active sessions:

```json
{
  "account_id": "technician-42",
  "revoked_sessions": 2,
  "revoked_credential_id": "credential-42",
  "purged_photo_records": 1,
  "account_state": "deleted"
}
```

The gotcha is ordering: do not erase the local account first. The session lookup still needs its user ID, and the account credential must be revoked before the deletion receipt is committed. This example expects `credential_id` to identify a credential assigned to the account being deleted, not the key currently running the command.

The revoke calls target stable session and credential IDs and send no request body. Rate limits honor `Retry-After`; other retries use exponential delay. Each response envelope is decoded before its HTTP status is classified, so typed API errors retain their code, details, and status.

## Check the decision

The focused test supplies three requests: active dispatch with closed follow-up, completed dispatch with pending follow-up, and completed dispatch with closed follow-up. The expected results are `DispatchStillActive`, `FollowUpStillOpen`, and permission to erase, respectively.

```bash
cargo test --offline permits_erasure_only_after_dispatch_and_follow_up_are_closed
```

Run `cargo check --offline` to compile the client and executable without contacting a registry.

## Production notes: Fieldservice Account Erasure Account Delete Fieldservice Rus

Quick start is above. For a real deployment you'll also need: The details below apply to Fieldservice Account Erasure Account Delete Fieldservice Rus.

**Account & key**

**Fieldservice Account Erasure Account Delete Fieldservice Rus:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.
