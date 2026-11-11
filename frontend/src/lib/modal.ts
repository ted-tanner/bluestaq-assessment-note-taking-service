// Native modal dialogs trap focus, make the background inert, and restore focus on close.
export function modal(element: HTMLDialogElement) {
  const previous = document.activeElement as HTMLElement | null;
  element.showModal();
  return {
    destroy() {
      element.close();
      previous?.focus();
    },
  };
}
