import React, { useState, useEffect, useRef, useMemo } from "react";
import {
  ShieldCheck,
  Search,
  Plus,
  Lock,
  Unlock,
  Check,
  Trash2,
  QrCode,
  Camera,
  Upload,
  X,
  ScanLine,
  Minus,
  Settings as SettingsIcon,
  HelpCircle,
  AlertTriangle,
  FileDown,
  FileUp,
  Sun,
  Moon,
  Laptop,
  Info,
  ArrowLeft,
  Heart,
  Globe,
  ExternalLink,
} from "lucide-react";
import {
  OtpAccount,
  CodeInfo,
  computeTotp,
  parseGoogleMigrationUri,
} from "./utils/totpClient";
import { scanQrFromFile, scanQrFromVideo, scanQrFromDataUrl } from "./utils/qrScanner";

// Safe invoke wrapper for Tauri
async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__) {
    const { invoke } = await import("@tauri-apps/api/core");
    return await invoke<T>(cmd, args);
  }
  throw new Error("Tauri runtime not detected");
}

async function openExternalUrl(url: string) {
  try {
    const { openUrl } = await import("@tauri-apps/plugin-opener");
    await openUrl(url);
  } catch {
    window.open(url, "_blank");
  }
}

interface AppSettings {
  showInDock: boolean;
  alwaysOnTop: boolean;
  closeOnBlur: boolean;
  openAtLogin: boolean;
  privacyMode: boolean;
  clipboardTimeoutSec: number;
  theme: "dark" | "light" | "system";
  accentColor: "monochrome" | "titanium" | "slate" | "graphite" | "onyx";
}

const DEFAULT_SETTINGS: AppSettings = {
  showInDock: false,
  alwaysOnTop: false,
  closeOnBlur: true,
  openAtLogin: true,
  privacyMode: false,
  clipboardTimeoutSec: 30,
  theme: "system",
  accentColor: "monochrome",
};

export function App() {
  // Accounts vault
  const [accounts, setAccounts] = useState<OtpAccount[]>(() => {
    const saved = localStorage.getItem("authg_accounts") || localStorage.getItem("authdesk_accounts");
    if (saved) {
      try {
        return JSON.parse(saved);
      } catch {
        return [];
      }
    }
    return [];
  });

  // Settings
  const [settings, setSettings] = useState<AppSettings>(() => {
    const saved = localStorage.getItem("authg_settings") || localStorage.getItem("authdesk_settings");
    if (saved) {
      try {
        const parsed = JSON.parse(saved);
        if (parsed.closeOnBlur === undefined) {
          parsed.closeOnBlur = true;
        }
        return { ...DEFAULT_SETTINGS, ...parsed };
      } catch {
        return DEFAULT_SETTINGS;
      }
    }
    return DEFAULT_SETTINGS;
  });

  const [codes, setCodes] = useState<Record<string, CodeInfo>>({});
  const [searchQuery, setSearchQuery] = useState("");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  // Modals
  const [isLocked, setIsLocked] = useState(false);
  const [pinInput, setPinInput] = useState("");
  const [vaultPin, setVaultPin] = useState<string>(() => {
    return localStorage.getItem("authg_pin") || "";
  });
  const [currentView, setCurrentView] = useState<"vault" | "settings" | "help">("vault");
  const [isImportModalOpen, setIsImportModalOpen] = useState(false);
  const [selectedAccount, setSelectedAccount] = useState<OtpAccount | null>(null);
  const [mirrorGoogleMode, setMirrorGoogleMode] = useState(false);

  // Modal active tabs
  const [activeImportTab, setActiveImportTab] = useState<"qr" | "manual">("qr");
  const [activeHelpTab, setActiveHelpTab] = useState<"google" | "shortcuts" | "faq">("google");

  // Manual Add Form
  const [manualIssuer, setManualIssuer] = useState("");
  const [manualName, setManualName] = useState("");
  const [manualSecret, setManualSecret] = useState("");
  const [manualDigits, setManualDigits] = useState(6);
  const [manualAlgo, setManualAlgo] = useState<"SHA1" | "SHA256" | "SHA512">("SHA1");

  // Migration URI input
  const [migrationUriText, setMigrationUriText] = useState("");

  // Change PIN form
  const [oldPinInput, setOldPinInput] = useState("");
  const [newPinInput, setNewPinInput] = useState("");

  // Camera Scanning
  const [isCameraActive, setIsCameraActive] = useState(false);
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const [isDraggingFile, setIsDraggingFile] = useState(false);

  // Global 30s Countdown
  const [secondsRemaining, setSecondsRemaining] = useState(30);

  // Apply Theme & Accent Color to root document
  useEffect(() => {
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: dark)");

    const applyTheme = () => {
      const isDark =
        settings.theme === "dark" ||
        (settings.theme === "system" && media.matches);

      if (isDark) {
        root.classList.remove("theme-light");
        root.classList.add("theme-dark");
      } else {
        root.classList.remove("theme-dark");
        root.classList.add("theme-light");
      }
    };

    applyTheme();
    root.setAttribute("data-accent", settings.accentColor || "monochrome");
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [settings.theme, settings.accentColor]);

  // Sync with native disk vault on boot
  useEffect(() => {
    tauriInvoke<boolean>("check_vault_exists")
      .then((exists) => {
        if (exists) {
          tauriInvoke<OtpAccount[]>("load_accounts_vault", { pin: vaultPin || "0000" })
            .then((loaded) => {
              if (loaded && loaded.length > 0) {
                setAccounts((curr) => (curr.length === 0 ? loaded : curr));
              }
            })
            .catch(() => {});
        }
      })
      .catch(() => {});
  }, [vaultPin]);

  // Save settings
  useEffect(() => {
    localStorage.setItem("authg_settings", JSON.stringify(settings));
    tauriInvoke("set_show_in_dock", { show: settings.showInDock }).catch(() => {});
    tauriInvoke("set_always_on_top", { alwaysOnTop: settings.alwaysOnTop }).catch(() => {});
    tauriInvoke("set_close_on_blur", { enable: settings.closeOnBlur }).catch(() => {});
    tauriInvoke("set_open_at_login", { enable: settings.openAtLogin }).catch(() => {});
  }, [settings]);

  // Save accounts
  useEffect(() => {
    localStorage.setItem("authg_accounts", JSON.stringify(accounts));
    tauriInvoke("save_accounts_vault", { pin: vaultPin, accounts }).catch(() => {});
  }, [accounts, vaultPin]);

  // Global hotkey / paste listener (⌘V) to automatically import QR images
  useEffect(() => {
    async function handleGlobalPaste(e: ClipboardEvent) {
      if (!e.clipboardData) return;
      const items = e.clipboardData.items;

      for (let i = 0; i < items.length; i++) {
        if (items[i].type.indexOf("image") !== -1) {
          const file = items[i].getAsFile();
          if (file) {
            try {
              const data = await scanQrFromFile(file);
              if (data) {
                handleScannedData(data);
                showToast("Imported QR from clipboard (⌘V)");
                return;
              }
            } catch {}
          }
        }
      }

      // Check text if migration URI
      const text = e.clipboardData.getData("text");
      if (text && text.startsWith("otpauth-migration://")) {
        handleScannedData(text);
        showToast("Imported migration URI from clipboard");
      }
    }

    window.addEventListener("paste", handleGlobalPaste);
    return () => window.removeEventListener("paste", handleGlobalPaste);
  }, []);

  // Prevent default browser navigation when dropping files into window
  useEffect(() => {
    const preventDrag = (e: DragEvent) => {
      e.preventDefault();
    };
    window.addEventListener("dragover", preventDrag);
    window.addEventListener("drop", preventDrag);
    return () => {
      window.removeEventListener("dragover", preventDrag);
      window.removeEventListener("drop", preventDrag);
    };
  }, []);

  // Listen to native Tauri file drop events (e.g. dropped from macOS Finder)
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const setupTauriDrop = async () => {
      if (typeof window !== "undefined" && (window as any).__TAURI_INTERNALS__) {
        try {
          const { getCurrentWebviewWindow } = await import("@tauri-apps/api/webviewWindow");
          const appWindow = getCurrentWebviewWindow();
          unlisten = await appWindow.onDragDropEvent(async (event) => {
            if (event.payload.type === "enter" || (event.payload as any).type === "over") {
              setIsDraggingFile(true);
            } else if (event.payload.type === "leave") {
              setIsDraggingFile(false);
            } else if (event.payload.type === "drop") {
              setIsDraggingFile(false);
              const paths = (event.payload as any).paths;
              if (paths && paths.length > 0) {
                const filePath = paths[0];
                try {
                  const dataUrl = await tauriInvoke<string>("read_image_base64", { path: filePath });
                  const data = await scanQrFromDataUrl(dataUrl);
                  if (data) {
                    handleScannedData(data);
                    showToast("Imported QR from dropped file");
                  } else {
                    showToast("No QR code found in dropped image");
                  }
                } catch {
                  showToast("Failed to read dropped file");
                }
              }
            }
          });
        } catch {
          // Ignore if Tauri drag-drop is unsupported in this environment
        }
      }
    };
    setupTauriDrop();
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  // Global key navigation (Escape to go back/close, ⌘K to search)
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === "Escape") {
        if (isImportModalOpen) {
          stopCamera();
          setIsImportModalOpen(false);
        } else if (selectedAccount) {
          setSelectedAccount(null);
        } else if (currentView !== "vault") {
          setCurrentView("vault");
        } else if (searchQuery) {
          setSearchQuery("");
        }
      } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        if (currentView !== "vault") {
          setCurrentView("vault");
        }
        setTimeout(() => {
          document.getElementById("search-accounts-input")?.focus();
        }, 50);
      }
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isImportModalOpen, selectedAccount, currentView, searchQuery]);


  // Periodic TOTP calculation
  useEffect(() => {
    let active = true;

    async function tick() {
      const nowSec = Math.floor(Date.now() / 1000);
      const remaining = 30 - (nowSec % 30);
      if (active) setSecondsRemaining(remaining);

      const computed: Record<string, CodeInfo> = {};
      for (const acc of accounts) {
        try {
          const res = await computeTotp(
            acc.secret,
            acc.algorithm,
            acc.digits,
            acc.period
          );
          computed[acc.id] = res;
        } catch {
          computed[acc.id] = {
            code: "------",
            secondsRemaining: 0,
            period: 30,
            progressPercent: 0,
          };
        }
      }
      if (active) {
        setCodes(computed);
      }
    }

    tick();
    const interval = setInterval(tick, 1000);
    return () => {
      active = false;
      clearInterval(interval);
    };
  }, [accounts]);

  // Camera QR scanner logic
  useEffect(() => {
    let animId: number;
    let isActive = true;

    if (isCameraActive) {
      const startCamera = async () => {
        try {
          if (!navigator.mediaDevices || !navigator.mediaDevices.getUserMedia) {
            showToast("Camera API not supported in this environment");
            setIsCameraActive(false);
            return;
          }

          let stream: MediaStream;
          try {
            // First attempt with ideal constraints (supports external or phone cameras)
            stream = await navigator.mediaDevices.getUserMedia({
              video: {
                facingMode: { ideal: "environment" },
                width: { ideal: 1280 },
                height: { ideal: 720 },
              },
            });
          } catch {
            // Fallback for Mac built-in FaceTime HD webcam (avoids OverconstrainedError)
            stream = await navigator.mediaDevices.getUserMedia({ video: true });
          }

          if (!isActive) {
            stream.getTracks().forEach((t) => t.stop());
            return;
          }

          streamRef.current = stream;
          if (videoRef.current) {
            videoRef.current.srcObject = stream;
            videoRef.current.setAttribute("playsinline", "true");
            await videoRef.current.play().catch(() => {});
          }

          const checkFrame = () => {
            if (!isActive) return;
            if (videoRef.current && videoRef.current.videoWidth > 0) {
              const scanned = scanQrFromVideo(videoRef.current);
              if (scanned) {
                handleScannedData(scanned);
                stopCamera();
                return;
              }
            }
            animId = requestAnimationFrame(checkFrame);
          };

          animId = requestAnimationFrame(checkFrame);
        } catch (err: any) {
          console.error("Camera access error:", err);
          if (err?.name === "NotAllowedError" || err?.name === "PermissionDeniedError") {
            showToast("Camera permission denied. Enable Camera in macOS System Settings > Privacy.");
          } else if (err?.name === "NotFoundError" || err?.name === "DevicesNotFoundError") {
            showToast("No camera detected on this machine.");
          } else {
            showToast(`Camera error: ${err?.message || err?.name || "Unavailable"}`);
          }
          setIsCameraActive(false);
        }
      };

      startCamera();
    }

    return () => {
      isActive = false;
      cancelAnimationFrame(animId);
      stopCamera();
    };
  }, [isCameraActive]);

  function stopCamera() {
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    }
    setIsCameraActive(false);
  }

  function showToast(msg: string) {
    setToastMessage(msg);
    setTimeout(() => setToastMessage(null), 2500);
  }

  async function copyCode(code: string, id: string) {
    const raw = code.replace(/\s+/g, "");
    if (raw === "------") return;

    try {
      await navigator.clipboard.writeText(raw);
      setCopiedId(id);
      const clearTime = settings.clipboardTimeoutSec;
      showToast(clearTime > 0 ? `Copied ${raw} (clears in ${clearTime}s)` : `Copied ${raw}`);
      setTimeout(() => setCopiedId(null), 1500);

      // Auto-clear clipboard
      if (clearTime > 0) {
        setTimeout(async () => {
          try {
            const current = await navigator.clipboard.readText();
            if (current === raw) {
              await navigator.clipboard.writeText("");
            }
          } catch {}
        }, clearTime * 1000);
      }
    } catch {
      showToast("Failed to copy code");
    }
  }

  function handleScannedData(data: string) {
    try {
      if (data.startsWith("otpauth-migration://")) {
        const imported = parseGoogleMigrationUri(data);
        if (imported.length > 0) {
          if (mirrorGoogleMode) {
            setAccounts(imported);
            showToast(`Mirrored ${imported.length} accounts (synced with phone)`);
          } else {
            setAccounts((prev) => {
              const existing = new Set(prev.map((a) => `${a.issuer}:${a.name}`.toLowerCase()));
              const fresh = imported.filter((a) => !existing.has(`${a.issuer}:${a.name}`.toLowerCase()));
              return [...fresh, ...prev];
            });
            showToast(`Imported ${imported.length} accounts`);
          }
          setIsImportModalOpen(false);
          return;
        }
      } else if (data.startsWith("otpauth://")) {
        const url = new URL(data);
        const secret = url.searchParams.get("secret") || "";
        const issuer = url.searchParams.get("issuer") || url.pathname.split(":")[0].replace("//totp/", "");
        const name = url.pathname.includes(":") ? url.pathname.split(":")[1] : url.pathname.replace("//totp/", "");

        if (secret) {
          const newAcc: OtpAccount = {
            id: `${issuer}-${name}-${Date.now()}`.toLowerCase().replace(/\s+/g, "-"),
            name: decodeURIComponent(name),
            issuer: decodeURIComponent(issuer),
            secret,
            algorithm: "SHA1",
            digits: 6,
            period: 30,
            otpType: "TOTP",
          };
          setAccounts((prev) => [newAcc, ...prev]);
          showToast(`Added ${newAcc.issuer}`);
          setIsImportModalOpen(false);
          return;
        }
      }
      showToast("Invalid QR code format");
    } catch (e: any) {
      showToast(e.message || "Failed to parse QR");
    }
  }

  async function handleFileUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;

    try {
      const data = await scanQrFromFile(file);
      if (data) {
        handleScannedData(data);
      } else {
        showToast("No QR code found in image");
      }
    } catch {
      showToast("Failed to read image file");
    }
  }

  async function handleDrop(e: React.DragEvent<HTMLLabelElement>) {
    e.preventDefault();
    e.stopPropagation();
    setIsDraggingFile(false);

    const file = e.dataTransfer.files?.[0];
    if (!file) return;

    try {
      const data = await scanQrFromFile(file);
      if (data) {
        handleScannedData(data);
        showToast("Imported QR from dropped image");
      } else {
        showToast("No QR code found in dropped image");
      }
    } catch {
      showToast("Failed to process dropped image file");
    }
  }

  function handleAddManual() {
    if (!manualSecret.trim() || !manualIssuer.trim()) {
      showToast("Issuer and secret key are required");
      return;
    }

    const newAcc: OtpAccount = {
      id: `${manualIssuer}-${manualName}-${Date.now()}`.toLowerCase().replace(/\s+/g, "-"),
      issuer: manualIssuer.trim(),
      name: manualName.trim() || "Account",
      secret: manualSecret.trim().toUpperCase(),
      algorithm: manualAlgo,
      digits: manualDigits,
      period: 30,
      otpType: "TOTP",
    };

    setAccounts((prev) => [newAcc, ...prev]);
    showToast(`Added ${newAcc.issuer}`);
    setManualIssuer("");
    setManualName("");
    setManualSecret("");
    setIsImportModalOpen(false);
  }

  function handleDeleteAccount(id: string) {
    setAccounts((prev) => prev.filter((a) => a.id !== id));
    showToast("Account deleted");
    setSelectedAccount(null);
  }

  function handleUnlock() {
    if (!vaultPin || pinInput === vaultPin) {
      setIsLocked(false);
      setPinInput("");
    } else {
      showToast("Incorrect passcode");
    }
  }

  function handleChangePin() {
    if (vaultPin && oldPinInput !== vaultPin) {
      showToast("Current passcode is incorrect");
      return;
    }
    if (newPinInput.length < 4) {
      showToast("New passcode must be at least 4 digits");
      return;
    }
    localStorage.setItem("authg_pin", newPinInput);
    setVaultPin(newPinInput);
    setOldPinInput("");
    setNewPinInput("");
    showToast("Master passcode updated");
  }

  function handleExportBackup() {
    const dataStr = JSON.stringify({
      version: "1.0",
      app: "AuthG",
      exportedAt: new Date().toISOString(),
      accounts,
    }, null, 2);

    const blob = new Blob([dataStr], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `authg_backup_${new Date().toISOString().split("T")[0]}.json`;
    a.click();
    URL.revokeObjectURL(url);
    showToast("Backup exported successfully");
  }

  function handleImportBackupFile(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (event) => {
      try {
        const parsed = JSON.parse(event.target?.result as string);
        const importedAccs: OtpAccount[] = Array.isArray(parsed) ? parsed : parsed.accounts;
        if (Array.isArray(importedAccs) && importedAccs.length > 0) {
          setAccounts((prev) => {
            const existing = new Set(prev.map((a) => `${a.issuer}:${a.name}`.toLowerCase()));
            const fresh = importedAccs.filter((a) => !existing.has(`${a.issuer}:${a.name}`.toLowerCase()));
            return [...fresh, ...prev];
          });
          showToast(`Restored ${importedAccs.length} accounts`);
          setCurrentView("vault");
        } else {
          showToast("No accounts found in backup file");
        }
      } catch {
        showToast("Invalid backup JSON file");
      }
    };
    reader.readAsText(file);
  }

  async function handleWipeVault() {
    if (confirm("DANGER: Wipe all accounts and delete local vault? This cannot be undone.")) {
      setAccounts([]);
      localStorage.removeItem("authg_accounts");
      localStorage.removeItem("authdesk_accounts");
      await tauriInvoke("wipe_vault").catch(() => {});
      showToast("Vault wiped clean");
      setCurrentView("vault");
    }
  }

  const filtered = useMemo(() => {
    const q = searchQuery.toLowerCase().trim();
    if (!q) return accounts;
    return accounts.filter(
      (a) =>
        a.issuer.toLowerCase().includes(q) ||
        a.name.toLowerCase().includes(q)
    );
  }, [accounts, searchQuery]);

  const tickerClass =
    secondsRemaining <= 5
      ? "urgent"
      : secondsRemaining <= 10
      ? "warning"
      : "safe";

  return (
    <div className="wm-window" id="authg-window">
      {/* Vault Locked Shield */}
      {isLocked ? (
        <div className="wm-modal-overlay">
          <div className="wm-modal" style={{ textAlign: "center", padding: "24px 20px" }}>
            <div style={{ margin: "0 auto 12px auto", width: 40, height: 40, borderRadius: 8, background: "var(--card)", border: "1px solid var(--border)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--foreground)" }}>
              <Lock size={18} />
            </div>
            <h3 style={{ fontSize: 14, fontWeight: 600, color: "var(--foreground)", marginBottom: 4 }}>
              Vault Locked
            </h3>
            <p style={{ fontSize: 11, color: "var(--muted)", marginBottom: 16 }}>
              Enter master passcode to view your 2FA codes
            </p>
            <input
              id="pin-unlock-input"
              type="password"
              maxLength={8}
              autoFocus
              className="wm-input mono"
              style={{ textAlign: "center", fontSize: 16, letterSpacing: 4, marginBottom: 12 }}
              placeholder="••••"
              value={pinInput}
              onChange={(e) => setPinInput(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && handleUnlock()}
            />
            <button id="unlock-submit-btn" className="wm-btn-primary" onClick={handleUnlock} style={{ width: "100%" }}>
              Unlock
            </button>
          </div>
        </div>
      ) : currentView === "settings" ? (
        <>
          {/* Settings Full Page Header */}
          <header className="wm-page-header" data-tauri-drag-region>
            <button
              id="settings-back-btn"
              className="wm-back-btn"
              onClick={() => setCurrentView("vault")}
            >
              <ArrowLeft size={14} />
              <span>Back</span>
            </button>
            <span className="wm-page-title">Settings</span>
            <div style={{ width: 52 }} />
          </header>

          {/* Settings Page Content */}
          <main className="wm-page-content" id="settings-page">
            {/* Section 1: Appearance & Theme */}
            <section>
              <div className="wm-section-title">Appearance & Theme</div>
              <div className="wm-section-card">
                <div className="wm-field">
                  <label className="wm-label">Theme Mode</label>
                  <div style={{ display: "flex", gap: 6, marginTop: 4 }}>
                    <button
                      id="theme-dark-btn"
                      className="wm-btn-secondary"
                      style={{
                        flex: 1,
                        borderColor: settings.theme === "dark" ? "var(--primary)" : undefined,
                        background: settings.theme === "dark" ? "var(--card-hover)" : undefined,
                      }}
                      onClick={() => setSettings((s) => ({ ...s, theme: "dark" }))}
                    >
                      <Moon size={12} /> Dark
                    </button>
                    <button
                      id="theme-light-btn"
                      className="wm-btn-secondary"
                      style={{
                        flex: 1,
                        borderColor: settings.theme === "light" ? "var(--primary)" : undefined,
                        background: settings.theme === "light" ? "var(--card-hover)" : undefined,
                      }}
                      onClick={() => setSettings((s) => ({ ...s, theme: "light" }))}
                    >
                      <Sun size={12} /> Light
                    </button>
                    <button
                      id="theme-system-btn"
                      className="wm-btn-secondary"
                      style={{
                        flex: 1,
                        borderColor: settings.theme === "system" ? "var(--primary)" : undefined,
                        background: settings.theme === "system" ? "var(--card-hover)" : undefined,
                      }}
                      onClick={() => setSettings((s) => ({ ...s, theme: "system" }))}
                    >
                      <Laptop size={12} /> System
                    </button>
                  </div>
                </div>

                <div className="wm-field" style={{ marginTop: 6 }}>
                  <label className="wm-label">Monochrome Palette</label>
                  <div className="wm-color-swatches">
                    {[
                      { id: "monochrome", color: "#ffffff", name: "Pure Monochrome" },
                      { id: "titanium", color: "#a1a1aa", name: "Titanium" },
                      { id: "slate", color: "#64748b", name: "Slate" },
                      { id: "graphite", color: "#71717a", name: "Graphite" },
                      { id: "onyx", color: "#3f3f46", name: "Onyx" },
                    ].map((swatch) => (
                      <div
                        key={swatch.id}
                        id={`swatch-${swatch.id}`}
                        className={`wm-swatch ${settings.accentColor === swatch.id ? "active" : ""}`}
                        style={{ backgroundColor: swatch.color }}
                        title={swatch.name}
                        onClick={() => setSettings((s) => ({ ...s, accentColor: swatch.id as any }))}
                      />
                    ))}
                  </div>
                </div>
              </div>
            </section>

            {/* Section 2: System & Window */}
            <section>
              <div className="wm-section-title">System & Window</div>
              <div className="wm-section-card">
                <div className="wm-setting-row">
                  <div className="wm-setting-info">
                    <span className="wm-setting-title">Keep Open in macOS Dock</span>
                    <span className="wm-setting-desc">Show app icon in Dock alongside Menu Bar</span>
                  </div>
                  <div
                    id="toggle-dock-btn"
                    className={`wm-toggle ${settings.showInDock ? "active" : ""}`}
                    onClick={() => setSettings((s) => ({ ...s, showInDock: !s.showInDock }))}
                  >
                    <div className="wm-toggle-thumb" />
                  </div>
                </div>

                <div className="wm-setting-row">
                  <div className="wm-setting-info">
                    <span className="wm-setting-title">Close on Outside Click</span>
                    <span className="wm-setting-desc">Dismiss window when clicking away like standard menu bar apps</span>
                  </div>
                  <div
                    id="toggle-close-blur-btn"
                    className={`wm-toggle ${settings.closeOnBlur ? "active" : ""}`}
                    onClick={() => setSettings((s) => ({ ...s, closeOnBlur: !s.closeOnBlur }))}
                  >
                    <div className="wm-toggle-thumb" />
                  </div>
                </div>

                <div className="wm-setting-row">
                  <div className="wm-setting-info">
                    <span className="wm-setting-title">Always on Top</span>
                    <span className="wm-setting-desc">Pin window above other apps when copying logins</span>
                  </div>
                  <div
                    id="toggle-always-top-btn"
                    className={`wm-toggle ${settings.alwaysOnTop ? "active" : ""}`}
                    onClick={() => setSettings((s) => ({ ...s, alwaysOnTop: !s.alwaysOnTop }))}
                  >
                    <div className="wm-toggle-thumb" />
                  </div>
                </div>

                <div className="wm-setting-row">
                  <div className="wm-setting-info">
                    <span className="wm-setting-title">Open at Login</span>
                    <span className="wm-setting-desc">Automatically launch AuthG into your Menu Bar when your Mac starts</span>
                  </div>
                  <div
                    id="toggle-autostart-btn"
                    className={`wm-toggle ${settings.openAtLogin ? "active" : ""}`}
                    onClick={() => setSettings((s) => ({ ...s, openAtLogin: !s.openAtLogin }))}
                  >
                    <div className="wm-toggle-thumb" />
                  </div>
                </div>

                <div className="wm-setting-row">
                  <div className="wm-setting-info">
                    <span className="wm-setting-title">Quick Access</span>
                    <span className="wm-setting-desc">Click menu bar icon or system tray anytime</span>
                  </div>
                  <span className="wm-badge">Menu Bar</span>
                </div>
              </div>
            </section>

            {/* Section 3: Security & Passcode */}
            <section>
              <div className="wm-section-title">Security & Passcode</div>
              <div className="wm-section-card">
                <div className="wm-setting-row">
                  <div className="wm-setting-info">
                    <span className="wm-setting-title">Privacy Mode</span>
                    <span className="wm-setting-desc">Blur 6-digit codes until hovered or clicked</span>
                  </div>
                  <div
                    id="toggle-privacy-btn"
                    className={`wm-toggle ${settings.privacyMode ? "active" : ""}`}
                    onClick={() => setSettings((s) => ({ ...s, privacyMode: !s.privacyMode }))}
                  >
                    <div className="wm-toggle-thumb" />
                  </div>
                </div>

                <div className="wm-field" style={{ marginTop: 6 }}>
                  <label className="wm-label">Clipboard Auto-Clear Timeout</label>
                  <select
                    id="select-clipboard-timeout"
                    className="wm-input"
                    value={settings.clipboardTimeoutSec}
                    onChange={(e) => setSettings((s) => ({ ...s, clipboardTimeoutSec: Number(e.target.value) }))}
                  >
                    <option value={15}>15 seconds</option>
                    <option value={30}>30 seconds (recommended)</option>
                    <option value={60}>60 seconds</option>
                    <option value={0}>Never clear</option>
                  </select>
                </div>

                {/* Change PIN section */}
                <div style={{ borderTop: "1px solid var(--border-subtle)", paddingTop: 10, marginTop: 6, display: "flex", flexDirection: "column", gap: 8 }}>
                  <div>
                    <span style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>Master Passcode</span>
                  </div>
                  <div style={{ display: "flex", flexDirection: "column", gap: 8, width: "100%" }}>
                    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                      <label style={{ fontSize: 11, fontWeight: 500, color: "var(--muted-foreground)" }}>Current Passcode</label>
                      <input
                        type="password"
                        placeholder="Enter current PIN"
                        className="wm-input mono"
                        style={{ width: "100%" }}
                        value={oldPinInput}
                        onChange={(e) => setOldPinInput(e.target.value)}
                      />
                    </div>
                    <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                      <label style={{ fontSize: 11, fontWeight: 500, color: "var(--muted-foreground)" }}>New Passcode</label>
                      <input
                        type="password"
                        placeholder="Enter new PIN"
                        className="wm-input mono"
                        style={{ width: "100%" }}
                        value={newPinInput}
                        onChange={(e) => setNewPinInput(e.target.value)}
                      />
                    </div>
                  </div>
                  <button className="wm-btn-secondary" style={{ width: "100%", justifyContent: "center" }} onClick={handleChangePin}>
                    Update Passcode
                  </button>
                </div>
              </div>
            </section>

            {/* Section 4: Data & Vault Backup */}
            <section>
              <div className="wm-section-title">Data & Vault Backup</div>
              <div className="wm-section-card">
                <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>Offline Backup & Restore</span>
                  <p style={{ fontSize: 11, color: "var(--muted)", lineHeight: 1.3 }}>
                    Export all accounts into an offline backup file, or restore from a previous export.
                  </p>
                  <div style={{ display: "flex", gap: 8, marginTop: 4 }}>
                    <button id="export-backup-btn" className="wm-btn-secondary" style={{ flex: 1 }} onClick={handleExportBackup}>
                      <FileDown size={13} /> Export JSON
                    </button>
                    <label
                      htmlFor="restore-file-input"
                      className="wm-btn-secondary"
                      style={{ flex: 1, cursor: "pointer" }}
                      onClick={() => tauriInvoke("set_prevent_close_on_blur", { prevent: true }).catch(() => {})}
                    >
                      <FileUp size={13} /> Restore File
                      <input
                        id="restore-file-input"
                        type="file"
                        accept=".json,.authg,.authdesk"
                        style={{ display: "none" }}
                        onChange={(e) => {
                          tauriInvoke("set_prevent_close_on_blur", { prevent: false }).catch(() => {});
                          handleImportBackupFile(e);
                        }}
                      />
                    </label>
                  </div>
                </div>

                <div className="wm-danger-zone" style={{ marginTop: 8 }}>
                  <div className="wm-danger-header">
                    <AlertTriangle size={14} />
                    <span>Danger Zone</span>
                  </div>
                  <p className="wm-danger-desc">
                    Wiping the vault permanently deletes all accounts and local encryption keys from this device.
                  </p>
                  <button
                    id="wipe-vault-btn"
                    className="wm-danger-btn"
                    onClick={handleWipeVault}
                  >
                    <Trash2 size={13} /> Wipe All Accounts
                  </button>
                </div>
              </div>
            </section>

            {/* Section 5: About & Support */}
            <section>
              <div className="wm-section-title">About & Support</div>
              <div className="wm-section-card">
                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", paddingBottom: 10, borderBottom: "1px solid var(--border)" }}>
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <div className="wm-logo-icon">
                      <ShieldCheck size={13} />
                    </div>
                    <div>
                      <div style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>AuthG v1.0.0</div>
                      <div style={{ fontSize: 10, color: "var(--muted)" }}>by Abhinav Dhakal</div>
                    </div>
                  </div>
                  <button
                    type="button"
                    className="wm-btn-secondary"
                    style={{ fontSize: 11, padding: "5px 10px", gap: 5 }}
                    onClick={() => openExternalUrl("https://authg.abhinavdhakal.com")}
                  >
                    <Globe size={12} /> Website <ExternalLink size={10} />
                  </button>
                </div>

                <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", paddingTop: 10 }}>
                  <div>
                    <div style={{ fontSize: 11, fontWeight: 500, color: "var(--foreground)" }}>Support Project</div>
                    <div style={{ fontSize: 10, color: "var(--muted)" }}>Buy me momo to support development</div>
                  </div>
                  <button
                    type="button"
                    className="wm-btn-secondary"
                    style={{ fontSize: 11, padding: "5px 10px", gap: 5, color: "var(--foreground)" }}
                    onClick={() => openExternalUrl("https://buymemomo.com/abhinavdhakal")}
                  >
                    <Heart size={12} className="wm-heart-icon" /> Support Me <ExternalLink size={10} />
                  </button>
                </div>
              </div>
            </section>
          </main>
        </>
      ) : currentView === "help" ? (
        <>
          {/* Help Full Page Header */}
          <header className="wm-page-header" data-tauri-drag-region>
            <button
              id="help-back-btn"
              className="wm-back-btn"
              onClick={() => setCurrentView("vault")}
            >
              <ArrowLeft size={14} />
              <span>Back</span>
            </button>
            <span className="wm-page-title">Help & Guides</span>
            <div style={{ width: 52 }} />
          </header>

          {/* Help Page Content */}
          <main className="wm-page-content" id="help-page">
            <div className="wm-tabs">
              <button
                id="tab-help-google"
                className={`wm-tab ${activeHelpTab === "google" ? "active" : ""}`}
                onClick={() => setActiveHelpTab("google")}
              >
                Google Export
              </button>
              <button
                id="tab-help-shortcuts"
                className={`wm-tab ${activeHelpTab === "shortcuts" ? "active" : ""}`}
                onClick={() => setActiveHelpTab("shortcuts")}
              >
                Shortcuts
              </button>
              <button
                id="tab-help-faq"
                className={`wm-tab ${activeHelpTab === "faq" ? "active" : ""}`}
                onClick={() => setActiveHelpTab("faq")}
              >
                FAQ
              </button>
            </div>

            {/* Tab 1: Google Export */}
            {activeHelpTab === "google" && (
              <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                <div className="wm-help-card">
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>
                    How to transfer Google Authenticator accounts:
                  </span>
                  
                  <div className="wm-help-step">
                    <div className="wm-step-num">1</div>
                    <div className="wm-step-content">
                      <span>Open <strong>Google Authenticator</strong> on your iOS or Android phone.</span>
                    </div>
                  </div>

                  <div className="wm-help-step">
                    <div className="wm-step-num">2</div>
                    <div className="wm-step-content">
                      <span>Tap the top-right menu icon and choose <strong style={{ color: "var(--foreground)" }}>Transfer accounts → Export accounts</strong>.</span>
                    </div>
                  </div>

                  <div className="wm-help-step">
                    <div className="wm-step-num">3</div>
                    <div className="wm-step-content">
                      <span>Verify your biometric security and select which accounts you wish to export. A large QR code will appear.</span>
                    </div>
                  </div>

                  <div className="wm-help-step">
                    <div className="wm-step-num">4</div>
                    <div className="wm-step-content">
                      <span>Take a screenshot/photo and <strong>press ⌘V anywhere in AuthG</strong>, or click the <strong>+</strong> button in the vault to upload or scan with camera.</span>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* Tab 2: Shortcuts */}
            {activeHelpTab === "shortcuts" && (
              <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div className="wm-section-card">
                  <div className="wm-setting-row">
                    <span className="wm-setting-title">Focus Search Bar</span>
                    <span className="wm-badge">⌘ + K</span>
                  </div>
                  <div className="wm-setting-row">
                    <span className="wm-setting-title">Paste QR Screenshot</span>
                    <span className="wm-badge">⌘ + V</span>
                  </div>
                  <div className="wm-setting-row">
                    <span className="wm-setting-title">Back to Vault / Close Dialog</span>
                    <span className="wm-badge">Esc</span>
                  </div>
                  <div className="wm-setting-row">
                    <span className="wm-setting-title">Copy 2FA Code</span>
                    <span className="wm-badge">Click Card</span>
                  </div>
                  <div className="wm-setting-row">
                    <span className="wm-setting-title">Account Details & Delete</span>
                    <span className="wm-badge">Right-click / (i)</span>
                  </div>
                  <div className="wm-setting-row">
                    <span className="wm-setting-title">Quick Access</span>
                    <span className="wm-badge">Menu Bar / Tray</span>
                  </div>
                </div>
              </div>
            )}

            {/* Tab 3: FAQ */}
            {activeHelpTab === "faq" && (
              <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                <div className="wm-help-card">
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>Will codes match Google Authenticator?</span>
                  <span style={{ fontSize: 11, color: "var(--muted)", lineHeight: 1.4 }}>
                    Yes, 100% identical. Both Google Authenticator and AuthG compute TOTP codes using the open RFC 6238 standard based on current UTC epoch time.
                  </span>
                </div>

                <div className="wm-help-card">
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>Are my 2FA secret keys sent to the cloud?</span>
                  <span style={{ fontSize: 11, color: "var(--muted)", lineHeight: 1.4 }}>
                    No. AuthG works 100% offline. No telemetry, no analytics, no external servers. Secrets are encrypted locally on disk with AES-256-GCM.
                  </span>
                </div>

                <div className="wm-help-card">
                  <span style={{ fontSize: 12, fontWeight: 600, color: "var(--foreground)" }}>What happens if I delete an account in Google Authenticator?</span>
                  <span style={{ fontSize: 11, color: "var(--muted)", lineHeight: 1.4 }}>
                    When re-exporting, enable "Mirror Google Authenticator" in the import dialog. It will synchronize your desktop vault to match your phone's current state.
                  </span>
                </div>

                <div style={{ marginTop: 14, paddingTop: 12, borderTop: "1px solid var(--border)", display: "flex", justifyContent: "space-between", alignItems: "center" }}>
                  <button
                    type="button"
                    className="wm-btn-secondary"
                    style={{ fontSize: 11, padding: "5px 9px", gap: 5 }}
                    onClick={() => openExternalUrl("https://authg.abhinavdhakal.com")}
                  >
                    <Globe size={12} /> authg.abhinavdhakal.com <ExternalLink size={10} />
                  </button>
                  <button
                    type="button"
                    className="wm-btn-secondary"
                    style={{ fontSize: 11, padding: "5px 9px", gap: 5, color: "var(--foreground)" }}
                    onClick={() => openExternalUrl("https://buymemomo.com/abhinavdhakal")}
                  >
                    <Heart size={12} className="wm-heart-icon" /> Support Me <ExternalLink size={10} />
                  </button>
                </div>
              </div>
            )}
          </main>
        </>
      ) : (
        <>
          {/* Main Vault Header */}
          <header className="wm-header" data-tauri-drag-region>
            <div className="wm-title" data-tauri-drag-region>
              <div className="wm-logo-icon">
                <ShieldCheck size={13} />
              </div>
              <span>AuthG</span>
            </div>

            <div className="wm-actions">
              <button
                id="help-btn"
                className="wm-btn-icon"
                title="Help & Guides"
                onClick={() => setCurrentView("help")}
              >
                <HelpCircle size={13} />
              </button>

              <button
                id="settings-btn"
                className="wm-btn-icon"
                title="Settings"
                onClick={() => setCurrentView("settings")}
              >
                <SettingsIcon size={13} />
              </button>

              <button
                id="toggle-lock-btn"
                className={`wm-btn-icon ${isLocked ? "active" : ""}`}
                title={isLocked ? "Unlock Vault" : "Lock Vault"}
                onClick={() => {
                  if (isLocked) {
                    setIsLocked(false);
                  } else if (!vaultPin) {
                    showToast("Set a master passcode in Settings to lock");
                    setCurrentView("settings");
                  } else {
                    setIsLocked(true);
                  }
                }}
              >
                {isLocked ? <Lock size={13} /> : <Unlock size={13} />}
              </button>

              <button
                id="open-import-btn"
                className="wm-btn-icon"
                title="Add or Import Account"
                onClick={() => setIsImportModalOpen(true)}
              >
                <Plus size={14} />
              </button>

              <button
                id="minimize-window-btn"
                className="wm-btn-icon"
                title="Minimize"
                onClick={() => tauriInvoke("hide_main_window").catch(() => {})}
              >
                <Minus size={13} />
              </button>
            </div>
          </header>

          {/* Search bar */}
          <div className="wm-search-wrapper">
            <div className="wm-input-group">
              <Search size={13} className="wm-search-icon" />
              <input
                id="search-accounts-input"
                type="text"
                autoFocus
                className="wm-search-input"
                placeholder="Search accounts (⌘K)..."
                value={searchQuery}
                onChange={(e) => setSearchQuery(e.target.value)}
              />
              {searchQuery && (
                <button
                  className="wm-btn-icon"
                  style={{ position: "absolute", right: 6, padding: 2 }}
                  onClick={() => setSearchQuery("")}
                >
                  <X size={11} />
                </button>
              )}
            </div>
          </div>

          {/* Sync & countdown bar */}
          <div className="wm-ticker-bar">
            <span>
              {filtered.length} {filtered.length === 1 ? "account" : "accounts"}
            </span>
            <div className="wm-ticker-pill">
              <div className={`wm-dot ${tickerClass}`} />
              <span>{secondsRemaining}s</span>
            </div>
          </div>

          {/* Account list or Empty State */}
          <main className="wm-list" id="account-cards-container">
            {filtered.length === 0 ? (
              <div className="wm-empty-state">
                <div className="wm-empty-icon-box">
                  <ScanLine size={22} />
                </div>
                <div>
                  <h4 className="wm-empty-title">
                    {searchQuery ? "No matching accounts" : "No 2FA accounts yet"}
                  </h4>
                  <p className="wm-empty-desc">
                    {searchQuery
                      ? `No accounts found for "${searchQuery}"`
                      : "Import your Google Authenticator export QR code, paste a screenshot with ⌘V, or add a key manually."}
                  </p>
                </div>
                {!searchQuery && (
                  <div style={{ display: "flex", gap: 8 }}>
                    <button
                      id="empty-import-action-btn"
                      className="wm-btn-primary"
                      onClick={() => setIsImportModalOpen(true)}
                    >
                      <Plus size={13} /> Import Accounts
                    </button>
                    <button
                      id="empty-help-btn"
                      className="wm-btn-secondary"
                      onClick={() => setCurrentView("help")}
                    >
                      <HelpCircle size={13} /> How it Works
                    </button>
                  </div>
                )}
              </div>
            ) : (
              filtered.map((acc) => {
                const info = codes[acc.id];
                const raw = info?.code || "------";
                const formatted =
                  raw.length === 6
                    ? `${raw.slice(0, 3)} ${raw.slice(3)}`
                    : raw.length === 8
                    ? `${raw.slice(0, 4)} ${raw.slice(4)}`
                    : raw;

                const isCopied = copiedId === acc.id;

                return (
                  <div
                    key={acc.id}
                    id={`account-card-${acc.id}`}
                    className={`wm-card ${isCopied ? "copied" : ""}`}
                    onClick={() => copyCode(raw, acc.id)}
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      setSelectedAccount(acc);
                    }}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      setSelectedAccount(acc);
                    }}
                    title="Click to copy • Double-click or right-click for details"
                  >
                    <div className="wm-card-left">
                      <div className="wm-account-info">
                        <span className="wm-account-issuer">{acc.issuer}</span>
                        <span className="wm-account-name">{acc.name}</span>
                      </div>
                    </div>

                    <div className="wm-card-right">
                      {isCopied ? (
                        <span className="wm-code-copied">
                          <Check size={14} />
                          <span>COPIED</span>
                        </span>
                      ) : (
                        <span
                          className={`wm-code ${secondsRemaining <= 5 ? "urgent" : ""}`}
                          style={{
                            filter: settings.privacyMode ? "blur(4px)" : "none",
                            transition: "filter 0.15s ease",
                          }}
                        >
                          {formatted}
                        </span>
                      )}
                      <button
                        id={`details-btn-${acc.id}`}
                        className="wm-btn-icon wm-card-info-btn"
                        title="Account details"
                        style={{ padding: 4 }}
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedAccount(acc);
                        }}
                      >
                        <Info size={13} />
                      </button>
                    </div>
                  </div>
                );
              })
            )}
          </main>
        </>
      )}


      {/* Import / Add Modal */}
      {isImportModalOpen && (
        <div className="wm-modal-overlay">
          <div className="wm-modal">
            <div className="wm-modal-header">
              <div className="wm-modal-title">
                <QrCode size={15} />
                <span>Import / Add Account</span>
              </div>
              <button
                id="close-import-modal-btn"
                className="wm-btn-icon"
                onClick={() => {
                  stopCamera();
                  setIsImportModalOpen(false);
                }}
              >
                <X size={14} />
              </button>
            </div>

            <div className="wm-modal-body">
              <div className="wm-tabs">
                <button
                  id="tab-qr-btn"
                  className={`wm-tab ${activeImportTab === "qr" ? "active" : ""}`}
                  onClick={() => {
                    stopCamera();
                    setActiveImportTab("qr");
                  }}
                >
                  Google Export QR
                </button>
                <button
                  id="tab-manual-btn"
                  className={`wm-tab ${activeImportTab === "manual" ? "active" : ""}`}
                  onClick={() => {
                    stopCamera();
                    setActiveImportTab("manual");
                  }}
                >
                  Manual Key
                </button>
              </div>

              {/* Tab 1: Google Authenticator QR */}
              {activeImportTab === "qr" && (
                <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                  <p style={{ fontSize: 11, color: "#71717a", lineHeight: 1.4 }}>
                    On your phone: open <strong>Google Authenticator → Transfer accounts → Export</strong>, then upload screenshot, scan with camera, or press <strong>⌘V</strong>:
                  </p>

                  {isCameraActive ? (
                    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                      <video
                        ref={videoRef}
                        style={{ width: "100%", height: 160, borderRadius: 6, background: "#000", objectFit: "cover" }}
                        autoPlay
                        playsInline
                        muted
                      />
                      <button className="wm-btn-secondary" onClick={stopCamera}>
                        Stop Camera
                      </button>
                    </div>
                  ) : (
                    <>
                      <label
                        htmlFor="qr-file-input"
                        className={`wm-dropzone ${isDraggingFile ? "dragging" : ""}`}
                        id="qr-dropzone-label"
                        onClick={() => tauriInvoke("set_prevent_close_on_blur", { prevent: true }).catch(() => {})}
                        onDragOver={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setIsDraggingFile(true);
                        }}
                        onDragEnter={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setIsDraggingFile(true);
                        }}
                        onDragLeave={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          setIsDraggingFile(false);
                        }}
                        onDrop={handleDrop}
                      >
                        <Upload size={20} color={isDraggingFile ? "var(--primary)" : "#a1a1aa"} />
                        <span style={{ fontSize: 12, fontWeight: 500, color: isDraggingFile ? "var(--primary)" : "var(--foreground)" }}>
                          {isDraggingFile ? "Release to import QR code" : "Drop QR screenshot or click to upload"}
                        </span>
                        <span style={{ fontSize: 10, color: "#71717a" }}>
                          Tip: You can also press ⌘V anytime to paste
                        </span>
                        <input
                          id="qr-file-input"
                          type="file"
                          accept="image/*"
                          style={{ display: "none" }}
                          onChange={(e) => {
                            tauriInvoke("set_prevent_close_on_blur", { prevent: false }).catch(() => {});
                            handleFileUpload(e);
                          }}
                        />
                      </label>

                      <button
                        id="start-camera-scan-btn"
                        className="wm-btn-secondary"
                        onClick={() => setIsCameraActive(true)}
                      >
                        <Camera size={13} /> Scan with Camera
                      </button>

                      <div className="wm-setting-row" style={{ padding: "6px 0", borderBottom: "none" }}>
                        <div className="wm-setting-info">
                          <span className="wm-setting-title" style={{ fontSize: 11 }}>Mirror Google Authenticator</span>
                          <span className="wm-setting-desc" style={{ fontSize: 10 }}>Replace vault with this export (prunes accounts you deleted on phone)</span>
                        </div>
                        <div
                          id="toggle-mirror-mode-btn"
                          className={`wm-toggle ${mirrorGoogleMode ? "active" : ""}`}
                          onClick={() => setMirrorGoogleMode(!mirrorGoogleMode)}
                        >
                          <div className="wm-toggle-thumb" />
                        </div>
                      </div>

                      <div className="wm-field" style={{ marginTop: 4 }}>
                        <label className="wm-label">Or paste otpauth-migration:// URI:</label>
                        <input
                          id="migration-paste-input"
                          type="text"
                          className="wm-input mono"
                          placeholder="otpauth-migration://offline?data=..."
                          value={migrationUriText}
                          onChange={(e) => setMigrationUriText(e.target.value)}
                        />
                        <button
                          id="import-pasted-url-btn"
                          className="wm-btn-primary"
                          style={{ marginTop: 4 }}
                          onClick={() => {
                            if (migrationUriText.trim()) {
                              handleScannedData(migrationUriText.trim());
                              setMigrationUriText("");
                            }
                          }}
                        >
                          Import Data
                        </button>
                      </div>
                    </>
                  )}
                </div>
              )}

              {/* Tab 2: Manual Key */}
              {activeImportTab === "manual" && (
                <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
                  <div className="wm-field">
                    <label className="wm-label">Service / Issuer</label>
                    <input
                      id="manual-issuer-input"
                      type="text"
                      className="wm-input"
                      placeholder="e.g. Google, GitHub, Cloudflare"
                      value={manualIssuer}
                      onChange={(e) => setManualIssuer(e.target.value)}
                    />
                  </div>

                  <div className="wm-field">
                    <label className="wm-label">Account Label / Email</label>
                    <input
                      id="manual-name-input"
                      type="text"
                      className="wm-input"
                      placeholder="user@example.com"
                      value={manualName}
                      onChange={(e) => setManualName(e.target.value)}
                    />
                  </div>

                  <div className="wm-field">
                    <label className="wm-label">Secret Key (Base32)</label>
                    <input
                      id="manual-secret-input"
                      type="text"
                      className="wm-input mono"
                      placeholder="JBSWY3DPEHPK3PXP"
                      value={manualSecret}
                      onChange={(e) => setManualSecret(e.target.value)}
                    />
                  </div>

                  <div style={{ display: "flex", gap: 8 }}>
                    <div className="wm-field" style={{ flex: 1 }}>
                      <label className="wm-label">Digits</label>
                      <select
                        id="manual-digits-select"
                        className="wm-input"
                        value={manualDigits}
                        onChange={(e) => setManualDigits(Number(e.target.value))}
                      >
                        <option value={6}>6 digits</option>
                        <option value={8}>8 digits</option>
                      </select>
                    </div>

                    <div className="wm-field" style={{ flex: 1 }}>
                      <label className="wm-label">Algorithm</label>
                      <select
                        id="manual-algo-select"
                        className="wm-input"
                        value={manualAlgo}
                        onChange={(e) => setManualAlgo(e.target.value as any)}
                      >
                        <option value="SHA1">SHA-1</option>
                        <option value="SHA256">SHA-256</option>
                        <option value="SHA512">SHA-512</option>
                      </select>
                    </div>
                  </div>

                  <button
                    id="manual-add-submit-btn"
                    className="wm-btn-primary"
                    style={{ marginTop: 6 }}
                    onClick={handleAddManual}
                  >
                    Add Account
                  </button>
                </div>
              )}
            </div>
          </div>
        </div>
      )}

      {/* Account Details & Safe Deletion Modal */}
      {selectedAccount && (
        <div className="wm-modal-overlay">
          <div className="wm-modal" style={{ maxWidth: 350 }}>
            <div className="wm-modal-header">
              <div className="wm-modal-title">
                <Info size={14} />
                <span>Account Details</span>
              </div>
              <button
                id="close-details-modal-btn"
                className="wm-btn-icon"
                onClick={() => setSelectedAccount(null)}
              >
                <X size={14} />
              </button>
            </div>

            <div className="wm-modal-body">
              <div className="wm-field">
                <label className="wm-label">Service / Issuer</label>
                <div style={{ fontSize: 13, fontWeight: 600, color: "var(--foreground)" }}>
                  {selectedAccount.issuer}
                </div>
              </div>

              <div className="wm-field">
                <label className="wm-label">Account Label / Email</label>
                <div style={{ fontSize: 12, color: "var(--muted-foreground)" }}>
                  {selectedAccount.name}
                </div>
              </div>

              <div style={{ display: "flex", gap: 12 }}>
                <div className="wm-field" style={{ flex: 1 }}>
                  <label className="wm-label">Algorithm</label>
                  <div style={{ fontSize: 11, color: "var(--muted-foreground)" }}>
                    {selectedAccount.algorithm || "SHA-1"}
                  </div>
                </div>
                <div className="wm-field" style={{ flex: 1 }}>
                  <label className="wm-label">Digits</label>
                  <div style={{ fontSize: 11, color: "var(--muted-foreground)" }}>
                    {selectedAccount.digits || 6} digits
                  </div>
                </div>
                <div className="wm-field" style={{ flex: 1 }}>
                  <label className="wm-label">Period</label>
                  <div style={{ fontSize: 11, color: "var(--muted-foreground)" }}>
                    {selectedAccount.period || 30}s
                  </div>
                </div>
              </div>

              {/* Danger Zone: Safe Delete */}
              <div className="wm-danger-zone" style={{ marginTop: 8 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 6, color: "var(--primary)" }}>
                  <AlertTriangle size={14} />
                  <span style={{ fontSize: 12, fontWeight: 600 }}>Danger Zone</span>
                </div>
                <p style={{ fontSize: 11, color: "var(--muted)", lineHeight: 1.3 }}>
                  Deleting this account removes its 2FA secret key. Make sure you have backup codes or alternative login methods.
                </p>
                <button
                  id="confirm-delete-account-btn"
                  className="wm-btn-secondary"
                  style={{ color: "var(--primary)", borderColor: "rgba(244, 63, 94, 0.4)" }}
                  onClick={() => {
                    if (confirm(`Are you sure you want to delete "${selectedAccount.issuer}: ${selectedAccount.name}"? You will lose access to 2FA codes for this service from AuthG.`)) {
                      handleDeleteAccount(selectedAccount.id);
                    }
                  }}
                >
                  <Trash2 size={13} /> Delete Account
                </button>
              </div>
            </div>
          </div>
        </div>
      )}

      {/* Toast */}
      {toastMessage && (
        <div className="wm-toast" id="app-toast">
          <Check size={12} />
          <span>{toastMessage}</span>
        </div>
      )}
    </div>
  );
}

export default App;
