import { Group, Panel, Separator } from "react-resizable-panels";
import { TopBar } from "./TopBar";
import { NavRail } from "./NavRail";
import { ProjectList } from "./ProjectList";
import { HomeView } from "./HomeView";
import { ProjectWorkspace } from "./ProjectWorkspace";
import { Inspector } from "./Inspector";
import { SettingsView } from "./SettingsView";
import { InboxView } from "./InboxView";
import { OrganizerView } from "./OrganizerView";
import { PendingVersionsView } from "./PendingVersionsView";
import { useUiStore } from "@/stores/ui-store";

interface Props {
  rootPath?: string | null;
  indexMessage?: string | null;
}

export function AppShell({ rootPath, indexMessage }: Props) {
  const view = useUiStore((s) => s.view);
  const inspectorOpen = useUiStore((s) => s.inspectorOpen);

  return (
    <div className="flex h-full flex-col">
      <TopBar rootPath={rootPath} indexMessage={indexMessage} />
      <div className="flex min-h-0 flex-1">
        <NavRail />
        <Group orientation="horizontal" className="min-w-0 flex-1">
          <Panel defaultSize={280} minSize={200} maxSize={420} className="min-w-0">
            <ProjectList />
          </Panel>
          <Separator className="w-px bg-border data-[separator-active]:bg-accent" />
          <Panel minSize={360} className="min-w-0">
            <main className="h-full min-w-0">
              {view.kind === "home" && <HomeView />}
              {view.kind === "settings" && <SettingsView />}
              {view.kind === "inbox" && <InboxView />}
              {view.kind === "organizer" && <OrganizerView />}
              {view.kind === "pending-versions" && <PendingVersionsView />}
              {view.kind === "project" && (
                <ProjectWorkspace
                  projectId={view.projectId}
                  categoryKey={view.categoryKey}
                />
              )}
            </main>
          </Panel>
          {inspectorOpen && (
            <>
              <Separator className="w-px bg-border data-[separator-active]:bg-accent" />
              <Panel defaultSize={360} minSize={280} maxSize={480} className="min-w-0">
                <Inspector />
              </Panel>
            </>
          )}
        </Group>
      </div>
    </div>
  );
}
