# Web client

The browser client will live in this directory. Its production assets will be
embedded in the `chat-server` binary.

`web/e2e/realtime.html` is a temporary browser harness for production-like
real-time verification. It is not the application client and should only be
served from the public chat origin during an E2E run.
