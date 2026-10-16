import type { ErrorBody } from './types.ts';

/** A non-2xx answer of the protocol: the HTTP status plus proto.Error. */
export class RpcError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = 'RpcError';
    this.status = status;
    this.code = code;
  }

  /** Builds an RpcError from a response body (JSON proto.Error or text). */
  static from(status: number, body: string): RpcError {
    try {
      const e = JSON.parse(body) as Partial<ErrorBody>;
      if (e && typeof e.error === 'string') {
        return new RpcError(status, e.code ?? '', e.error);
      }
    } catch {
      // not JSON: fall through
    }
    return new RpcError(status, '', body.trim() || `HTTP ${status}`);
  }
}

/** Whether err is an RpcError with the given code (like rpc.IsCode in Go). */
export function isCode(err: unknown, code: string): boolean {
  return err instanceof RpcError && err.code === code;
}
