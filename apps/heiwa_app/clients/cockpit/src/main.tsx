import { Route, Router } from "@solidjs/router";
import { render } from "solid-js/web";
import App from "./App";
import Agents from "./routes/Agents";
import Approvals from "./routes/Approvals";
import Cells from "./routes/Cells";
import Connections from "./routes/Connections";
import Crons from "./routes/Crons";
import Dashboard from "./routes/Dashboard";
import Domains from "./routes/Domains";
import Governance from "./routes/Governance";
import History from "./routes/History";
import Hooks from "./routes/Hooks";
import Inbox from "./routes/Inbox";
import Live from "./routes/Live";
import Memory from "./routes/Memory";
import Missions from "./routes/Missions";
import NotFound from "./routes/NotFound";
import Providers from "./routes/Providers";
import RateGroups from "./routes/RateGroups";
import Repl from "./routes/Repl";
import Routes from "./routes/Routes";
import Status from "./routes/Status";
import Traces from "./routes/Traces";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Dashboard} />
      <Route path="/providers" component={Providers} />
      <Route path="/connections" component={Connections} />
      <Route path="/inbox" component={Inbox} />
      <Route path="/routes" component={Routes} />
      <Route path="/live" component={Live} />
      <Route path="/repl" component={Repl} />
      <Route path="/missions" component={Missions} />
      <Route path="/approvals" component={Approvals} />
      <Route path="/history" component={History} />
      <Route path="/traces" component={Traces} />
      <Route path="/memory" component={Memory} />
      <Route path="/agents" component={Agents} />
      <Route path="/hooks" component={Hooks} />
      <Route path="/crons" component={Crons} />
      <Route path="/rate-groups" component={RateGroups} />
      <Route path="/cells" component={Cells} />
      <Route path="/status" component={Status} />
      <Route path="/domains" component={Domains} />
      <Route path="/governance" component={Governance} />
      <Route path="*" component={NotFound} />
    </Router>
  ),
  root,
);
