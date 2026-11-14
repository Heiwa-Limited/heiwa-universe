import type { SVGProps } from "react";

export function HeiwaWordmark(props: SVGProps<SVGSVGElement>) {
  return (
    <svg {...props} viewBox="0 0 92 28" xmlns="http://www.w3.org/2000/svg">
      <text
        x="0"
        y="22"
        fill="currentColor"
        fontFamily="system-ui, sans-serif"
        fontSize="27"
        fontWeight="600"
        letterSpacing="-1.1"
      >
        Heiwa
      </text>
    </svg>
  );
}
