import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import App from "./App";
import Dashboard from "./routes/Dashboard";
import Providers from "./routes/Providers";
import Connections from "./routes/Connections";
import Routes from "./routes/Routes";
import Live from "./routes/Live";
import Repl from "./routes/Repl";
import Missions from "./routes/Missions";
import Approvals from "./routes/Approvals";
import History from "./routes/History";
import Traces from "./routes/Traces";
import Memory from "./routes/Memory";
import Agents from "./routes/Agents";
import Crons from "./routes/Crons";
import RateGroups from "./routes/RateGroups";
import Cells from "./routes/Cells";
import Status from "./routes/Status";
import Domains from "./routes/Domains";
import Governance from "./routes/Governance";
import NotFound from "./routes/NotFound";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Dashboard} />
      <Route path="/providers" component={Providers} />
      <Route path="/connections" component={Connections} />
      <Route path="/routes" component={Routes} />
      <Route path="/live" component={Live} />
      <Route path="/repl" component={Repl} />
      <Route path="/missions" component={Missions} />
      <Route path="/approvals" component={Approvals} />
      <Route path="/history" component={History} />
      <Route path="/traces" component={Traces} />
      <Route path="/memory" component={Memory} />
      <Route path="/agents" component={Agents} />
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
