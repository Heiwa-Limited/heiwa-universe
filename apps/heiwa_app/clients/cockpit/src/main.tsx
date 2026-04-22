import { render } from "solid-js/web";
import { Router, Route } from "@solidjs/router";
import App from "./App";
import Dashboard from "./routes/Dashboard";
import Providers from "./routes/Providers";
import Routes from "./routes/Routes";
import Repl from "./routes/Repl";
import NotFound from "./routes/NotFound";

const root = document.getElementById("root");
if (!root) throw new Error("#root not found");

render(
  () => (
    <Router root={App}>
      <Route path="/" component={Dashboard} />
      <Route path="/providers" component={Providers} />
      <Route path="/routes" component={Routes} />
      <Route path="/repl" component={Repl} />
      <Route path="*" component={NotFound} />
    </Router>
  ),
  root,
);
