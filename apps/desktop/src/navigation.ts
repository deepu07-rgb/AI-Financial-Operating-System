export const routeIds = [
  "home",
  "inbox",
  "review",
  "invoices",
  "parties",
  "payments",
  "reports",
  "advisor",
  "integrations",
  "pilot",
  "settings",
] as const;

export type RouteId = (typeof routeIds)[number];

export interface NavigationRoute {
  id: RouteId;
  path: string;
}

export const navigationRoutes: readonly NavigationRoute[] = [
  { id: "home", path: "/" },
  { id: "inbox", path: "/inbox" },
  { id: "review", path: "/review" },
  { id: "invoices", path: "/invoices" },
  { id: "parties", path: "/parties" },
  { id: "payments", path: "/payments" },
  { id: "reports", path: "/reports" },
  { id: "advisor", path: "/advisor" },
  { id: "integrations", path: "/integrations" },
  { id: "pilot", path: "/help" },
  { id: "settings", path: "/settings" },
];

export const mainNavigationRoutes = navigationRoutes.filter(
  (route) => route.id !== "pilot" && route.id !== "settings",
);

export const utilityNavigationRoutes = navigationRoutes.filter(
  (route) => route.id === "pilot" || route.id === "settings",
);

const routesByPath = new Map(
  navigationRoutes.map((route) => [route.path, route.id]),
);
const routesById = new Map(
  navigationRoutes.map((route) => [route.id, route.path]),
);

export function routeFromPathname(pathname: string): RouteId {
  const normalizedPath =
    pathname.length > 1 ? pathname.replace(/\/+$/u, "") : pathname;
  if (normalizedPath === "/pilot") return "pilot";
  return routesByPath.get(normalizedPath) ?? "home";
}

export function pathForRoute(routeId: RouteId): string {
  return routesById.get(routeId) ?? "/";
}
