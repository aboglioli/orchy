export type ApiErrorPayload = {
  code: string;
  message: string;
  details?: unknown;
};

export type ApiErrorEnvelope = {
  error: ApiErrorPayload;
};

export type ApiSuccessEnvelope<T> = {
  data: T;
};
