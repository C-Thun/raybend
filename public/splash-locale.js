/* Tauri 在每个文档加载前注入：同一次应用启动共用启动前的语言快照。 */
(function () {
  var locale = "en-US";
  var launch = window.__RAYBEND_LAUNCH_ID__;
  var snapshotKey = "raybend.splash-launch.v1";
  try {
    var storage = window.localStorage;
    function capturedLocale() {
      var snapshot = null;
      try { snapshot = JSON.parse(storage.getItem(snapshotKey)); } catch (_) {}
      return snapshot && snapshot.launch === launch &&
        (snapshot.locale === "zh-CN" || snapshot.locale === "en-US") ? snapshot.locale : null;
    }
    var captured = capturedLocale();
    if (captured) {
      locale = captured;
    } else {
      locale = storage.getItem("raybend.locale") === "zh-CN" ? "zh-CN" : "en-US";
      // 另一个 WebView 可能在前两次读之间完成快照并初始化主界面语言，优先它的启动值。
      captured = capturedLocale();
      if (captured) locale = captured;
      // 仅是每次启动的派生快照，下次启动换 launch 后重新读取唯一的语言设置。
      else try { storage.setItem(snapshotKey, JSON.stringify({ launch: launch, locale: locale })); } catch (_) {}
    }
  } catch (_) {
    // 存储不可用时也能立即显示英文图。
  }
  window.__RAYBEND_SPLASH_LOCALE__ = locale;
})();
