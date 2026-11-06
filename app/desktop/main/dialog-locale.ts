import type { RuntimeConnection } from "../contracts.ts";

const messages = {
  en: {
    cancel: "Cancel",
    open: "Open in browser",
    share: "Choose a screen or window to share",
    stopped: "momo runtime stopped",
    reconnect:
      "Restart momo to reconnect. No activities will resume automatically.",
    failed: "momo could not start",
    help: "Check that this installation contains its runtime resources and that its data directory is not in use.",
    retry: "Retry",
    quit: "Quit",
  },
  zh: {
    cancel: "取消",
    open: "在浏览器中打开",
    share: "选择要共享的屏幕或窗口",
    stopped: "momo 运行服务已停止",
    reconnect: "请重新启动 momo 以恢复连接。活动不会自动恢复。",
    failed: "momo 无法启动",
    help: "请检查安装是否包含运行资源，并确认数据目录未被其他进程占用。",
    retry: "重试",
    quit: "退出",
  },
  ja: {
    cancel: "キャンセル",
    open: "ブラウザーで開く",
    share: "共有する画面またはウィンドウを選択",
    stopped: "momo の実行サービスが停止しました",
    reconnect:
      "momo を再起動して再接続してください。アクティビティは自動的に再開されません。",
    failed: "momo を起動できませんでした",
    help: "必要な実行リソースがインストールされていることと、データディレクトリが使用中でないことを確認してください。",
    retry: "再試行",
    quit: "終了",
  },
};
export type DialogTexts = typeof messages.en;
export function dialogLocale(locale: string): keyof typeof messages {
  const language = locale.toLowerCase().split(/[-_]/)[0];
  return language === "zh" || language === "ja" ? language : "en";
}

// Only the last language is cached; credentials and state remain ephemeral.
export function createDialogTexts(
  osLocale: string,
  connection: () => RuntimeConnection | undefined,
  request: typeof fetch = fetch,
  timeoutMs = 750,
): () => Promise<DialogTexts> {
  let language = dialogLocale(osLocale);
  return async () => {
    const runtime = connection();
    if (runtime) {
      try {
        const response = await request(`${runtime.baseUrl}/api/state`, {
          headers: { Authorization: `Bearer ${runtime.token}` },
          redirect: "error",
          signal: AbortSignal.timeout(timeoutMs),
        });
        if (response.ok) {
          const state = await response.json();
          const selected = state?.settings?.general?.lang;
          if (selected === "zh" || selected === "en" || selected === "ja")
            language = selected;
        }
      } catch {
        /* Backend unavailable: retain the last known UI language. */
      }
    }
    return messages[language];
  };
}
