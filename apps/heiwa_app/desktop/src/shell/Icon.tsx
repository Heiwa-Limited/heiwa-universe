import type { JSX } from "solid-js";

export function Icon(props: { name: "home" | "sessions" | "calendar" | "mail" | "search" | "plus" | "send" | "more" | "chevron" | "heiwa" | "close" | "folder"; size?: number }) {
  const size = props.size ?? 18;
  const paths: Record<string, JSX.Element> = {
    home: <><path d="m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1Z" /></>,
    sessions: <><path d="M5 5h14v10H9l-4 4v-4H5Z" /><path d="M8 9h8M8 12h5" /></>,
    calendar: <><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M7 3v4M17 3v4M3 10h18" /></>,
    mail: <><rect x="3" y="5" width="18" height="14" rx="2" /><path d="m4 7 8 6 8-6" /></>,
    search: <><circle cx="11" cy="11" r="6" /><path d="m16 16 4 4" /></>,
    plus: <path d="M12 5v14M5 12h14" />,
    send: <path d="M12 19V5m-6 6 6-6 6 6" />,
    heiwa: <><path d="m12 2 8.5 5v10L12 22l-8.5-5V7Z" /><path d="m3.5 7 8.5 5 8.5-5M12 12v10M12 2v10L3.5 17m8.5-5 8.5 5" /></>,
    close: <path d="m6 6 12 12M6 18 18 6" />,
    folder: <path d="M3 7V5h6l2 2h10v13H3Z" />,
    more: <path d="M5 12h.01M12 12h.01M19 12h.01" />,
    chevron: <path d="m9 18 6-6-6-6" />,
  };
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">{paths[props.name]}</svg>;
}
