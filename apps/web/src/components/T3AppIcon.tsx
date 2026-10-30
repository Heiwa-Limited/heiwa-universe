// The production icon, as GitHub shows the Heiwa app, whatever this build's channel.
import appIconUrl from "../../../../assets/heiwa/icon.png";

export function T3AppIcon(props: { readonly className?: string }) {
  return <img src={appIconUrl} alt="" className={props.className} />;
}
