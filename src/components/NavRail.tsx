import {
  Home,
  FolderKanban,
  Archive,
  Settings,
  Inbox,
  FolderTree,
  GitBranch,
} from "lucide-react";
import { cn } from "@/lib/utils";
import { useUiStore } from "@/stores/ui-store";

interface NavRailProps {
  /** Icon-only column for the collapsible left rail. */
  compact?: boolean;
}

export function NavRail({ compact = false }: NavRailProps) {
  const view = useUiStore((s) => s.view);
  const setView = useUiStore((s) => s.setView);
  const showArchived = useUiStore((s) => s.showArchived);
  const setShowArchived = useUiStore((s) => s.setShowArchived);

  const items = [
    {
      id: "home",
      label: "Inicio",
      icon: Home,
      active: view.kind === "home",
      onClick: () => setView({ kind: "home" }),
    },
    {
      id: "projects",
      label: "Proyectos",
      icon: FolderKanban,
      active: view.kind === "project",
      onClick: () => setView({ kind: "home" }),
    },
    {
      id: "inbox",
      label: "Bandeja",
      icon: Inbox,
      active: view.kind === "inbox",
      onClick: () => setView({ kind: "inbox" }),
    },
    {
      id: "organizer",
      label: "Organizador",
      icon: FolderTree,
      active: view.kind === "organizer",
      onClick: () => setView({ kind: "organizer" }),
    },
    {
      id: "pending-versions",
      label: "Versiones",
      icon: GitBranch,
      active: view.kind === "pending-versions",
      onClick: () => setView({ kind: "pending-versions" }),
    },
    {
      id: "archived",
      label: "Archivados",
      icon: Archive,
      active: showArchived,
      onClick: () => setShowArchived(!showArchived),
    },
    {
      id: "settings",
      label: "Ajustes",
      icon: Settings,
      active: view.kind === "settings",
      onClick: () => setView({ kind: "settings" }),
    },
  ];

  if (compact) {
    return (
      <nav
        className="flex min-h-0 flex-1 flex-col"
        aria-label="Navegación"
      >
        <ul className="flex flex-col items-center gap-0.5 py-2">
          {items.map((item) => (
            <li key={item.id}>
              <button
                type="button"
                onClick={item.onClick}
                title={item.label}
                aria-label={item.label}
                aria-current={item.active ? "page" : undefined}
                className={cn(
                  "flex h-8 w-8 items-center justify-center rounded-[var(--radius-sm)]",
                  item.active
                    ? "bg-selection text-text"
                    : "text-muted hover:bg-elevated hover:text-text",
                )}
              >
                <item.icon size={16} />
              </button>
            </li>
          ))}
        </ul>
      </nav>
    );
  }

  return (
    <nav className="flex w-[220px] shrink-0 flex-col border-r border-border bg-panel">
      <div className="px-3 py-3 text-[11px] font-semibold uppercase tracking-wider text-muted">
        Navegación
      </div>
      <ul className="flex flex-col gap-0.5 px-2">
        {items.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              onClick={item.onClick}
              className={cn(
                "flex h-[38px] w-full items-center gap-2.5 rounded-[var(--radius-sm)] px-2.5 text-left text-[13px]",
                item.active
                  ? "bg-selection text-text"
                  : "text-muted hover:bg-elevated hover:text-text",
              )}
            >
              <item.icon size={16} />
              {item.label}
            </button>
          </li>
        ))}
      </ul>

      <div className="mt-auto border-t border-border p-3 text-[11px] text-muted">
        IngManager · organizador / bandeja / versiones
      </div>
    </nav>
  );
}
