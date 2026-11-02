// Capability observation only; never a performance workload or a substitute API.
print(JSON.stringify({
  document:typeof document,
  window:typeof window,
  TextEncoder:typeof TextEncoder,
  TextDecoder:typeof TextDecoder,
  ReadableStream:typeof ReadableStream,
  TransformStream:typeof TransformStream,
  AbortController:typeof AbortController,
  MessageChannel:typeof MessageChannel,
  Request:typeof Request,
  Response:typeof Response,
  Headers:typeof Headers,
  URL:typeof URL,
  fetch:typeof fetch,
  crypto:typeof crypto
}));
