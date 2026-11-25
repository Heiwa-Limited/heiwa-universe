import {
  AlertTriangleIcon,
  BellIcon,
  CalendarIcon,
  CheckIcon,
  ClockIcon,
  DollarSignIcon,
  InfoIcon,
  MailIcon,
  MessageSquareIcon,
  TrendingDownIcon,
  TrendingUpIcon,
} from "lucide-react";
import { type KeyboardEvent, type ReactNode, useState } from "react";

import { cn } from "../../lib/utils";
import { Button } from "../ui/button";
import { Input } from "../ui/input";
import { Separator } from "../ui/separator";
import {
  type A2uiActionEvent,
  type A2uiNode,
  type DataModel,
  actionEvent,
  bindingPath,
  buttonOrder,
  indexSurface,
  resolveText,
  resolveValue,
  writePointer,
} from "./a2ui.logic";

/** Basic-catalog icon names we map; anything else falls back to an info glyph. */
const ICONS: Record<string, typeof InfoIcon> = {
  alert: AlertTriangleIcon,
  bell: BellIcon,
  calendar: CalendarIcon,
  check: CheckIcon,
  clock: ClockIcon,
  dollar: DollarSignIcon,
  mail: MailIcon,
  message: MessageSquareIcon,
  trending_down: TrendingDownIcon,
  trending_up: TrendingUpIcon,
};

const TEXT_CLASS: Record<string, string> = {
  h1: "text-lg font-semibold",
  h2: "text-base font-semibold",
  h3: "text-sm font-semibold",
  h4: "text-sm font-medium",
  h5: "text-sm font-medium",
  body: "text-sm font-medium",
  caption: "text-xs text-muted-foreground",
};

export interface A2uiSurfaceProps {
  readonly surface: {
    readonly createSurface: {
      readonly surfaceId: string;
      readonly components: ReadonlyArray<Readonly<Record<string, unknown>>>;
    };
  };
  readonly dataModel?: DataModel;
  readonly onAction: (event: A2uiActionEvent) => void;
  readonly disabled?: boolean;
  readonly className?: string;
}

/**
 * Renders one A2UI v1.0 surface with Heiwa's components. Supports the basic catalog's
 * layout, text, icon, divider, button and simple inputs bound to a local data model;
 * other components render as a labelled placeholder rather than failing the card.
 * A focused surface answers with 1–9: the nth button, in reading order.
 */
export function A2uiSurface({
  surface,
  dataModel,
  onAction,
  disabled,
  className,
}: A2uiSurfaceProps) {
  const { surfaceId, components } = surface.createSurface;
  const { nodes, problems } = indexSurface(components);
  const order = buttonOrder(nodes);
  const [data, setData] = useState<DataModel>(dataModel ?? {});

  if (problems.length > 0) {
    return (
      <div
        role="alert"
        className={cn(
          "rounded-lg border border-dashed p-3 text-xs text-muted-foreground",
          className,
        )}
      >
        This card could not be shown: {problems[0]}
      </div>
    );
  }

  const fire = (node: A2uiNode) => {
    const event = actionEvent(node, surfaceId, data);
    if (event && !disabled) onAction(event);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const index = Number(event.key) - 1;
    const target = Number.isInteger(index) && index >= 0 ? order[index] : undefined;
    const node = target ? nodes.get(target) : undefined;
    if (!node) return;
    event.preventDefault();
    fire(node);
  };

  const bind = (node: A2uiNode, key: string) => ({
    value: resolveValue(node[key], data),
    set: (value: unknown) => {
      const path = bindingPath(node[key]);
      if (path) setData((current) => writePointer(current, path, value));
    },
  });

  const render = (id: string): ReactNode => {
    const node = nodes.get(id)!;
    switch (node.component) {
      case "Card":
        return (
          <div key={id} className="rounded-xl border bg-card p-4 shadow-xs/5">
            {render(node.child as string)}
          </div>
        );
      case "Column":
      case "List":
        return (
          <div key={id} className="flex flex-col gap-1.5">
            {(node.children as string[]).map(render)}
          </div>
        );
      case "Row":
        return (
          <div key={id} className="mt-2 flex flex-wrap items-center gap-2">
            {(node.children as string[]).map(render)}
          </div>
        );
      case "Text": {
        const variant = typeof node.variant === "string" ? node.variant : "body";
        return (
          <p key={id} className={cn(TEXT_CLASS[variant] ?? TEXT_CLASS.body, "text-pretty")}>
            {resolveText(node.text, data)}
          </p>
        );
      }
      case "Icon": {
        const Glyph = ICONS[resolveText(node.name, data)] ?? InfoIcon;
        return <Glyph key={id} aria-hidden className="size-4 text-muted-foreground" />;
      }
      case "Divider":
        return <Separator key={id} className="my-1" />;
      case "Button": {
        const position = order.indexOf(id);
        const label = nodes.get(node.child as string);
        const primary = position === 0 || node.variant === "primary";
        return (
          <Button
            key={id}
            size="sm"
            variant={primary ? "default" : "outline"}
            disabled={disabled}
            onClick={() => fire(node)}
            title={position >= 0 && position < 9 ? `Shortcut: ${position + 1}` : undefined}
          >
            {label ? resolveText(label.text, data) : "Choose"}
            {position >= 0 && position < 9 ? (
              <kbd className="ms-1 hidden rounded border px-1 font-mono text-3xs opacity-60 sm:inline">
                {position + 1}
              </kbd>
            ) : null}
          </Button>
        );
      }
      case "TextField": {
        const field = bind(node, "value");
        return (
          <label key={id} className="flex flex-col gap-1 text-xs text-muted-foreground">
            {resolveText(node.label, data)}
            <Input
              value={typeof field.value === "string" ? field.value : ""}
              placeholder={resolveText(node.placeholder, data)}
              onChange={(event) => field.set(event.currentTarget.value)}
            />
          </label>
        );
      }
      case "CheckBox": {
        const field = bind(node, "value");
        return (
          <label key={id} className="flex items-center gap-2 text-sm">
            <input
              type="checkbox"
              className="size-4 accent-primary"
              checked={field.value === true}
              onChange={(event) => field.set(event.currentTarget.checked)}
            />
            {resolveText(node.label, data)}
          </label>
        );
      }
      case "Image": {
        const url = resolveText(node.url, data);
        // Only https and inline images: a surface must not make the client fetch plain http.
        return /^(https:|data:image\/)/.test(url) ? (
          <img
            key={id}
            src={url}
            alt={resolveText(node.altText, data)}
            className="max-h-48 rounded-md"
          />
        ) : null;
      }
      default:
        return (
          <span
            key={id}
            className="rounded border border-dashed px-1.5 py-0.5 text-2xs text-muted-foreground"
          >
            {node.component}
          </span>
        );
    }
  };

  return (
    <div
      className={cn(
        "rounded-xl outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
        className,
      )}
      tabIndex={0}
      onKeyDown={onKeyDown}
      data-surface-id={surfaceId}
    >
      {render("root")}
    </div>
  );
}
