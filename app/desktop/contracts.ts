export interface RuntimeConnection {
  baseUrl: string;
  token: string;
}
export interface DesktopBridge {
  getRuntime(): Promise<RuntimeConnection>;
}
