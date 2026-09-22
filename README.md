# Pebble Bounce

Simple Cloudflare worker that accepts webhooks from a Pebble Index 01 and converts them to JSON, forwarding the `Authorization` header the Index sends with them. This is useful for passing along requests from the Index to services like AI agents that accept webhooks but only in JSON form (e.g. Grok Bot).

The Index request is in a `multipart/form-data` see the ([docs](https://help.repebble.com/en/articles/15724406-index-advanced-features-mcp-webhook)) here.

## Usage

- `git clone` the repo to your PC
- Optional: `nix develop` for a shell with wrangler, plus a Rust toolchain of your own (rustup) — the build compiles the worker to wasm, and `worker-build` pulls the nightly toolchain it needs on first run
- `npx wrangler login` if you have not authenticated wrangler before
- `npx wrangler deploy` to deploy to your Cloudflare account, this can run on the free plan easily
- `npx wrangler secret put BOUNCE_URL` add the webhook URL for whatever you are bouncing to, or add it in the Cloudflare dashboard. The worker answers `500` until this is set
- In the Index webhook settings, point the URL at your worker and set an `Authorization` header. The header is required: without it the worker replies `401` and forwards nothing
- Send your requests

## Example JSON translated from an Index webhook

```json
{
  "recordedAt": "1790047766895",
  "client": "ring",
  "transcription": "remember the milk",
  "audio": {
    "name": "clip.m4a",
    "type": "audio/mp4",
    "size": 106152,
    "data": "<base64 of the m4a>"
  }
}
```

`recordedAt` is milliseconds since the unix epoch, as text. `transcription` and `audio` are `null` when the Index is not configured to send them, so the shape is the same every time. The audio is base64 because JSON cannot carry the raw m4a bytes.

## What comes back

The destination's reply is returned as-is: its status code, its body, and its `Content-Type`, `WWW-Authenticate` and `Location` headers. A `401` from the destination reaches the Index as a `401`, so a bad token looks like a bad token rather than a worker bug.

Everything else the worker can answer:

| Status | When |
| --- | --- |
| `401` | No `Authorization` header on the request |
| `405` | Not a POST |
| `411` | No `Content-Length` (a chunked body cannot be size-checked) |
| `413` | Body over 25 MiB |
| `400` | Body is not `multipart/form-data` |
| `500` | `BOUNCE_URL` secret is not set |
| `502` | Destination unreachable; details go to the worker log, not the response |

A redirect from the destination is handed back rather than followed, so the recording is never replayed to a host the destination names.

## Note on auth

The worker does not check the `Authorization` header, it only requires one to be present and passes it on. Anyone who learns the worker's URL can post to it with any token, and your destination is the only thing deciding what is valid — so make sure it rejects what it does not recognise. A Cloudflare rate-limiting or WAF rule on the route can be used to protect it if you are so inclined.

## Local development

```sh
nix develop
cp .dev.vars.example .dev.vars   # set BOUNCE_URL to a local endpoint
wrangler dev
```
