import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Widget } from "./Widget";
import { Details } from "./details/Details";
import "./styles.css";

// One bundle, two windows: the label Tauri gives each window picks its screen.
const label = getCurrentWindow().label;
document.documentElement.dataset.window = label;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{label === "widget" ? <Widget /> : <Details />}</React.StrictMode>,
);
