<script lang="ts">
  import { goto } from "$app/navigation";
  import {
    api,
    ApiError,
    json,
    message,
    type Challenge,
    type User,
  } from "#lib/api.js";
  import { session, signedIn } from "#lib/session.svelte.js";
  import { onMount, tick } from "svelte";
  import {
    ArrowRight,
    ArrowLeft,
    Eye,
    EyeOff,
    Mail,
    ShieldCheck,
    KeyRound,
  } from "@lucide/svelte";
  let {
    mode = "signin",
    reauth = false,
    oncomplete,
  }: {
    mode?: "signin" | "register" | "reset";
    reauth?: boolean;
    oncomplete?: () => void;
  } = $props();
  let email = $state("");
  let name = $state("");
  let password = $state("");
  let code = $state("");
  let challenge = $state<Challenge | null>(null);
  let codeInput = $state<HTMLInputElement>();
  $effect(() => {
    const challengeId = challenge?.challenge_id;
    if (challengeId)
      void tick().then(() => {
        if (challenge?.challenge_id === challengeId) codeInput?.focus();
      });
  });
  let busy = $state(false);
  let error = $state("");
  let visible = $state(false);
  let success = $state(false);
  let now = $state(Date.now());
  let expiresAt = $state(0);
  let resendAt = $state(0);
  const remaining = $derived(Math.max(0, Math.ceil((resendAt - now) / 1000)));
  const expired = $derived(challenge !== null && now >= expiresAt);
  onMount(() => {
    const timer = setInterval(() => (now = Date.now()), 1000);
    return () => clearInterval(timer);
  });
  function acceptChallenge(next: Challenge) {
    challenge = next;
    code = "";
    password = "";
    now = Date.now();
    expiresAt = now + next.expires_in * 1000;
    resendAt = now + 60000;
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = "";
    if (
      (mode === "register" && !challenge) ||
      (mode === "reset" && challenge)
    ) {
      if (
        [...password].length < 12 ||
        new TextEncoder().encode(password).length > 1024
      ) {
        error =
          "Use at least 12 characters and no more than 1,024 bytes for your password.";
        return;
      }
    }
    busy = true;
    try {
      if (!challenge) {
        acceptChallenge(
          await api<Challenge>(
            mode === "register"
              ? "/auth/register"
              : mode === "reset"
                ? "/auth/password-reset/request"
                : "/auth/sign-in",
            {
              method: "POST",
              body: json(
                mode === "reset"
                  ? { email }
                  : mode === "register"
                    ? { email, password, name: name.trim() }
                    : { email, password },
              ),
            },
            false,
          ),
        );
      } else if (mode === "reset") {
        await api(
          "/auth/password-reset/confirm",
          {
            method: "POST",
            body: json({
              challenge_id: challenge.challenge_id,
              code: code.trim(),
              new_password: password,
            }),
          },
          false,
        );
        password = "";
        code = "";
        challenge = null;
        success = true;
      } else {
        const result = await api<{ user: User }>(
          challenge.purpose === "registration"
            ? "/auth/register/verify"
            : "/auth/2fa/email/verify",
          {
            method: "POST",
            body: json({
              challenge_id: challenge.challenge_id,
              code: code.trim(),
            }),
          },
          false,
        );
        if (reauth && session.user && result.user.id !== session.user.id) {
          await api("/auth/logout", { method: "POST" }, false);
          error = "Sign in with the original account to recover this draft.";
          challenge = null;
          return;
        }
        password = "";
        code = "";
        challenge = null;
        signedIn(result.user);
        if (oncomplete) oncomplete();
        else await goto("/notes", { replace: true });
      }
    } catch (e) {
      error =
        e instanceof ApiError && e.status === 401
          ? challenge
            ? "That code is incorrect, expired, or already used. Try again or request a new code."
            : "The email or password is incorrect. Try again or reset your password."
          : message(e);
    } finally {
      busy = false;
    }
  }
  async function resend() {
    if (!challenge || busy || remaining) return;
    busy = true;
    error = "";
    try {
      acceptChallenge(
        await api<Challenge>(
          "/auth/email/resend",
          {
            method: "POST",
            body: json({ challenge_id: challenge.challenge_id }),
          },
          false,
        ),
      );
    } catch (e) {
      error = message(e);
    } finally {
      busy = false;
    }
  }
</script>

<div class="auth-form">
  <div class="auth-icon">
    {#if challenge}<Mail size={25} />{:else if mode === "reset"}<KeyRound
        size={25}
      />{:else}<ShieldCheck size={25} />{/if}
  </div>
  <h1>
    {success
      ? "Password updated"
      : challenge
        ? "Check your email"
        : reauth
          ? "Welcome back"
          : mode === "register"
            ? "A little space for your ideas."
            : mode === "reset"
              ? "Let’s get you back in."
              : "Good to see you again."}
  </h1>
  <p class="muted auth-description">
    {success
      ? "Your password has been reset. Sign in with your new password and email verification code."
      : challenge
        ? mode === "reset"
          ? `If an account exists for ${email}, we’ve sent a recovery code. It expires in 10 minutes.`
          : `Enter the 8-character code sent to ${email}. It expires in 10 minutes.`
        : reauth
          ? "Your session expired. Sign in with the same account to keep working on your draft."
          : mode === "register"
            ? "Create your account and give your thoughts a place to grow."
            : mode === "reset"
              ? "Enter your email to request a password reset code."
              : "Sign in to pick up where you left off"}
  </p>
  {#if success}<a class="button primary full" href="/sign-in"
      >Back to sign in <ArrowRight size={17} /></a
    >
  {:else}
    <form onsubmit={submit}>
      {#if !challenge}
        {#if mode === "register"}<label for="name">Full name</label><input
            id="name"
            autocomplete="name"
            required
            maxlength="100"
            bind:value={name}
            disabled={busy}
            placeholder="Your name"
          />{/if}
        <label for="email">Email address</label><input
          id="email"
          type="email"
          autocomplete="username"
          required
          bind:value={email}
          disabled={busy}
          placeholder="you@example.com"
        />
      {:else}
        <label for="code">Verification code</label><input
          id="code"
          bind:this={codeInput}
          class="code-input"
          autocomplete="one-time-code"
          autocapitalize="characters"
          spellcheck="false"
          pattern={"[A-Za-z0-9]{8}"}
          maxlength="8"
          required
          bind:value={code}
          disabled={busy}
          placeholder="XXXXXXXX"
          aria-describedby="code-help"
        />
        <p id="code-help" class="field-help">
          {expired
            ? "This code has expired. Request a new code below."
            : "Eight letters and numbers. You can paste the whole code."}
        </p>
      {/if}
      {#if (mode !== "reset" && !challenge) || (mode === "reset" && challenge)}
        <div class="label-row">
          <label for="password"
            >{mode === "reset" ? "New password" : "Password"}</label
          >{#if mode === "signin" && !reauth}<a href="/recover"
              >Forgot password?</a
            >{/if}
        </div>
        <div class="password-field">
          <input
            id="password"
            type={visible ? "text" : "password"}
            autocomplete={mode === "signin"
              ? "current-password"
              : "new-password"}
            required
            bind:value={password}
            disabled={busy}
            placeholder={mode === "signin"
              ? "Your password"
              : "At least 12 characters"}
          /><button
            type="button"
            class="icon-button"
            aria-label={visible ? "Hide password" : "Show password"}
            onclick={() => (visible = !visible)}
            >{#if visible}<EyeOff size={18} />{:else}<Eye
                size={18}
              />{/if}</button
          >
        </div>
        {#if mode !== "signin"}<p class="field-help">
            Use at least 12 characters. A long, unique passphrase works well.
          </p>{/if}
      {/if}
      {#if error}<p class="notice danger" role="alert">{error}</p>{/if}
      <button class="primary full" disabled={busy || expired}
        >{busy
          ? "Please wait…"
          : challenge
            ? mode === "reset"
              ? "Reset password"
              : "Verify & continue"
            : mode === "reset"
              ? "Send recovery code"
              : mode === "register"
                ? "Create account"
                : "Sign in"}<ArrowRight size={17} /></button
      >
    </form>
    {#if challenge}<div class="challenge-actions">
        <button
          class="text-button"
          disabled={busy || remaining > 0}
          onclick={resend}
          >{remaining ? `Resend in ${remaining}s` : "Send a new code"}</button
        ><button
          class="text-button"
          disabled={busy}
          onclick={() => {
            challenge = null;
            code = "";
            password = "";
            error = "";
          }}><ArrowLeft size={14} /> Use another email</button
        >
      </div>
    {:else if !reauth}<p class="auth-switch">
        {mode === "register"
          ? "Already have an account?"
          : mode === "reset"
            ? "Remember your password?"
            : "New here?"}
        <a href={mode === "signin" ? "/register" : "/sign-in"}
          >{mode === "signin" ? "Create an account" : "Sign in"}</a
        >
      </p>{/if}
  {/if}
  <p class="auth-security">
    <ShieldCheck size={14} />
    {mode === "reset"
      ? "Recovery always requires email verification."
      : "Protected with email verification at every sign-in."}
  </p>
</div>
