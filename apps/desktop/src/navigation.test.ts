import { describe, expect, it } from "vitest";
import {
  navigationRoutes,
  pathForRoute,
  routeFromPathname,
} from "./navigation";

describe("desktop navigation", () => {
  it("round-trips every declared route", () => {
    for (const route of navigationRoutes) {
      expect(routeFromPathname(pathForRoute(route.id))).toBe(route.id);
    }
  });

  it("accepts trailing slashes and safely falls back home", () => {
    expect(routeFromPathname("/reports/")).toBe("reports");
    expect(routeFromPathname("/help")).toBe("pilot");
    expect(routeFromPathname("/pilot")).toBe("pilot");
    expect(routeFromPathname("/not-a-real-route")).toBe("home");
  });
});
