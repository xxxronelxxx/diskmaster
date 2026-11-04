/**
 * Backwards-compatible module name: the UI still imports `queryClient` and
 * `rpcClient` from here, while the implementation lives in `api.ts`.
 */
export { api, queryClient, queryClient as rpcClient } from "./api";
