// Keep long titles readable while typing and when the viewport changes width.
export function autosize(element: HTMLTextAreaElement, _value: string) {
  const resize = () => {
    element.style.height = "auto";
    element.style.height = `${element.scrollHeight}px`;
  };
  resize();
  let width = element.clientWidth;
  let frame = 0;
  const observer = new ResizeObserver(() => {
    if (element.clientWidth === width) return;
    width = element.clientWidth;
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(resize);
  });
  observer.observe(element);
  return {
    update: resize,
    destroy() {
      observer.disconnect();
      cancelAnimationFrame(frame);
    },
  };
}
