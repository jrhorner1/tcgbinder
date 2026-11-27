// Dialogs: open any dialog component and wait for its answer. DialogHost (in App.svelte)
// draws the open ones, so several can stack (a question over the Add dialog).
//
// A dialog component takes a `done(value)` prop and calls it with its answer; closing
// it (Escape, Cancel) answers null.

import type { Component } from "svelte";
import Ask from "../components/Ask.svelte";
import AskName from "../components/AskName.svelte";
import { app } from "./app.svelte";

interface Open { id: number; component: Component<any>; props: Record<string, unknown>; done: (v: unknown) => void }

export const dialogs = $state<Open[]>([]);
let nextId = 1;

/** Opens `component` with `props` and resolves to what it answers (null if closed). */
export function openDialog<T>(component: Component<any>, props: Record<string, unknown> = {}): Promise<T | null> {
  return new Promise((resolve) => {
    const id = nextId++;
    const done = (v: unknown) => {
      const i = dialogs.findIndex((d) => d.id === id);
      if (i >= 0) dialogs.splice(i, 1);
      resolve((v ?? null) as T | null);
    };
    dialogs.push({ id, component, props, done });
  });
}

export interface Choice<T> { label: string; value: T; primary?: boolean; danger?: boolean }

/** Asks a question with buttons; resolves to the chosen button's value, or null. With
 *  `cancel` false there's no Cancel (a message to acknowledge). */
export const askDialog = <T>(title: string, message: string | string[], buttons: Choice<T>[], cancel = true) =>
  openDialog<T>(Ask, { title, message: [message].flat(), buttons, cancel });

/** A yes/no question in the app's own dialog. (The apps' web views answer
 *  window.confirm() with "no" without showing it.) */
export const confirmDialog = (message: string, { danger = false, action = "OK", title = "Are you sure?" } = {}) =>
  askDialog(title, message, [{ label: action, value: true, primary: !danger, danger }]).then((v) => v === true);

/** Confirms a deletion, unless Settings turned that off. */
export const ask = async (message: string, action = "Delete") =>
  !app.settings?.confirm_delete || confirmDialog(message, { danger: true, action });

/** Asks for a name (a list's, a tag's); null if cancelled. */
export const askName = (title: string, value = "", placeholder = "e.g. Mono-red burn") =>
  openDialog<string>(AskName, { title, value, placeholder });
