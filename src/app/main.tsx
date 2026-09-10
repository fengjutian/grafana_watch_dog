import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { MantineProvider, createTheme } from "@mantine/core";
import { Notifications } from "@mantine/notifications";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import App from "./App";
import "@mantine/core/styles.css";
import "@mantine/notifications/styles.css";
import "../styles.css";

const queryClient = new QueryClient({ defaultOptions: { queries: { staleTime: 30_000, retry: 1 } } });
const theme = createTheme({ primaryColor: "teal", fontFamily: "DM Sans, Microsoft YaHei, sans-serif", headings: { fontFamily: "Manrope, Microsoft YaHei, sans-serif" }, defaultRadius: "md" });

createRoot(document.getElementById("root")!).render(
  <StrictMode><QueryClientProvider client={queryClient}><MantineProvider theme={theme}><Notifications position="bottom-right" /><App /></MantineProvider></QueryClientProvider></StrictMode>
);
