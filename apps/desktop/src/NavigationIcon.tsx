import {
  Bot,
  Building2,
  CircleDollarSign,
  FileCheck2,
  FileInput,
  FileText,
  Gauge,
  HandCoins,
  HelpCircle,
  Inbox,
  Settings,
  type LucideProps,
} from "lucide-react";
import type { ReactNode } from "react";
import type { RouteId } from "./navigation";

const icons: Record<RouteId, (props: LucideProps) => ReactNode> = {
  advisor: (props) => <Bot {...props} />,
  home: (props) => <Gauge {...props} />,
  inbox: (props) => <Inbox {...props} />,
  integrations: (props) => <FileInput {...props} />,
  invoices: (props) => <FileText {...props} />,
  parties: (props) => <Building2 {...props} />,
  payments: (props) => <HandCoins {...props} />,
  pilot: (props) => <HelpCircle {...props} />,
  reports: (props) => <FileCheck2 {...props} />,
  review: (props) => <CircleDollarSign {...props} />,
  settings: (props) => <Settings {...props} />,
};

export function NavigationIcon({
  routeId,
  ...props
}: LucideProps & { routeId: RouteId }) {
  return icons[routeId](props);
}
