import { api, ApiError, type User } from "./api";
export const session = $state<{
  user: User | null;
  ready: boolean;
  expired: boolean;
  error: string;
}>({ user: null, ready: false, expired: false, error: "" });
export async function initialize() {
  try {
    session.user = await api<User>("/user/me");
    session.error = "";
  } catch (error) {
    if (!(error instanceof ApiError && error.status === 401))
      session.error = "Unable to reach the service. Please retry.";
  } finally {
    session.ready = true;
  }
}
export function signedIn(user: User) {
  session.user = user;
  session.expired = false;
  session.error = "";
  const channel = new BroadcastChannel("bluestaq-session");
  channel.postMessage({ type: "account", id: user.id });
  channel.close();
}
