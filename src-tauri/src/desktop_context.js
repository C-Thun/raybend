(() => {
  const key = "__raybendDefaultContextMenuSuppressed";
  if (window[key]) return;
  window[key] = true;
  window.addEventListener("contextmenu", (event) => event.preventDefault(), true);
})();
