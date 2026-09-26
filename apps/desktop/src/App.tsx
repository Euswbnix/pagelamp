import { useState } from "react";
import { RouterProvider } from "react-router";
import type { PageLampApi } from "@/api/client";
import { Providers } from "@/app/Providers";
import { createAppRouter } from "@/app/router";

export function App({ api }: { api: PageLampApi }) {
  const [router] = useState(createAppRouter);
  return (
    <Providers api={api}>
      <RouterProvider router={router} />
    </Providers>
  );
}
