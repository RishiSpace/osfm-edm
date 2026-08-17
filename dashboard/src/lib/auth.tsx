"use client";

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { useRouter } from "next/navigation";
import { ApiError, get, post, refreshAccessToken, setAccessToken } from "./api";
import type { User } from "./types";

type AuthContextValue = {
  user: User | null;
  ready: boolean;
  isAdmin: boolean;
  login: (username: string, password: string, totp?: string) => Promise<void>;
  logout: () => Promise<void>;
  reload: () => Promise<void>;
};

const AuthContext = createContext<AuthContextValue | null>(null);

export function AuthProvider({ children }: { children: React.ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [ready, setReady] = useState(false);

  const reload = useCallback(async () => {
    const me = await get<User>("/api/v1/auth/me");
    setUser(me);
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const token = await refreshAccessToken();
        if (token && !cancelled) {
          await reload();
        }
      } catch {
        setUser(null);
      } finally {
        if (!cancelled) setReady(true);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [reload]);

  const login = useCallback(
    async (username: string, password: string, totp?: string) => {
      const data = await post<{ access_token: string }>("/api/v1/auth/login", {
        username,
        password,
        totp_code: totp || undefined,
      });
      setAccessToken(data.access_token);
      await reload();
    },
    [reload],
  );

  const logout = useCallback(async () => {
    try {
      await post("/api/v1/auth/logout");
    } catch (err) {
      if (!(err instanceof ApiError)) throw err;
    }
    setAccessToken(null);
    setUser(null);
  }, []);

  const value = useMemo<AuthContextValue>(
    () => ({
      user,
      ready,
      isAdmin: user?.role === "admin",
      login,
      logout,
      reload,
    }),
    [user, ready, login, logout, reload],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (!ctx) throw new Error("useAuth must be used within AuthProvider");
  return ctx;
}

export function useRequireAuth(): AuthContextValue {
  const auth = useAuth();
  const router = useRouter();
  useEffect(() => {
    if (auth.ready && !auth.user) {
      router.replace("/login");
    }
  }, [auth.ready, auth.user, router]);
  return auth;
}
