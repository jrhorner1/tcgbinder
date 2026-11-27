// Starts the UI: draws App.svelte into the page.
import { mount } from "svelte";
import "./app.css";
import App from "./App.svelte";

mount(App, { target: document.getElementById("app")! });
