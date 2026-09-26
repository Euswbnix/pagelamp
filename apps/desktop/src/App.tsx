import { useState } from "react";
import { RouterProvider } from "react-router";
import type { WeekmarkApi } from "@/api/client";
import { Providers } from "@/app/Providers";
import { createAppRouter } from "@/app/router";

export function App({ api }: { api: WeekmarkApi }) {
  const [router] = useState(createAppRouter);
  return (
    <Providers api={api}>
      <RouterProvider router={router} />
    </Providers>
  );
}
