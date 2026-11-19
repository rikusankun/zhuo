export interface HookStatus {
  isLoading: boolean;
  error: string | null;
  success: string | null;
}

export interface HookReturn<TData, TActions> {
  data: TData;
  actions: TActions;
  status: HookStatus;
}
