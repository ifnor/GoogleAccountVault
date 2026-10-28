<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Lock,
  Unlock,
  Search,
  Plus,
  Copy,
  Check,
  ExternalLink,
  Globe,
  CreditCard,
  Trash2,
  Edit3,
  Eye,
  EyeOff,
  Folder,
  Sparkles,
  Clock,
  Info,
  AlertCircle,
  AlertTriangle,
  CheckCircle2,
  Compass,
  Upload,
  Cloud,
  History,
  Download,
  KeyRound,
  RefreshCw,
  FileText,
  Fingerprint,
  Settings,
  ChevronLeft,
  MoreVertical,
  Zap,
  Shield,
  X
} from "lucide-vue-next";

interface AccountRecord {
  id: string;
  email: string;
  password?: string;
  group?: string;
  status?: string;
  recovery_email?: string;
  phone?: string;
  totp?: string;
  verification?: string;
  backup_codes?: string;
  used_backup_codes?: string;
  notes?: string;
  google_one_plan?: string;
  google_one_used?: string;
  google_one_status?: string;
  updated_at?: number;
  deleted_at?: number | null;
  [key: string]: any;
}

interface TotpResult {
  code: string;
  remaining_seconds: number;
}

// App State
const isUnlocked = ref(false);
const vaultExists = ref(true);
const masterPassword = ref("");
const unlockError = ref("");
const isSubmitting = ref(false);

const records = ref<AccountRecord[]>([]);
const selectedId = ref<string | null>(null);
const searchQuery = ref("");
const currentGroup = ref<string | null>(null);

// Responsive Mobile View States
const mobileView = ref<"list" | "detail">("list");
const showMobileSearch = ref(false);
const showMobileMoreMenu = ref(false);

const autoSelectFirst = ref(localStorage.getItem("vault_auto_select_first") === "true");
function toggleAutoSelectFirst() {
  autoSelectFirst.value = !autoSelectFirst.value;
  localStorage.setItem("vault_auto_select_first", autoSelectFirst.value ? "true" : "false");
}

function selectAccount(id: string) {
  if (selectedId.value === id) {
    selectedId.value = null;
    mobileView.value = "list";
  } else {
    selectedId.value = id;
    mobileView.value = "detail";
  }
}

// Detail Panel Width & Resizing (Desktop)
const detailWidth = ref<number>(
  Math.max(280, Math.min(500, Number(localStorage.getItem("vault_detail_panel_width") || 340)))
);
const isResizingDetail = ref(false);
const isDesktopWindow = ref(typeof window !== "undefined" ? window.innerWidth >= 768 : true);

function updateWindowWidth() {
  if (typeof window !== "undefined") {
    isDesktopWindow.value = window.innerWidth >= 768;
  }
}

function startResizeDetail(e: MouseEvent) {
  e.preventDefault();
  isResizingDetail.value = true;
  const startX = e.clientX;
  const startWidth = detailWidth.value;

  function onMouseMove(moveEvent: MouseEvent) {
    const delta = startX - moveEvent.clientX;
    const newWidth = Math.max(280, Math.min(500, startWidth + delta));
    detailWidth.value = newWidth;
  }

  function onMouseUp() {
    isResizingDetail.value = false;
    localStorage.setItem("vault_detail_panel_width", String(detailWidth.value));
    window.removeEventListener("mousemove", onMouseMove);
    window.removeEventListener("mouseup", onMouseUp);
  }

  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp);
}

function resetDetailWidth() {
  detailWidth.value = 340;
  localStorage.setItem("vault_detail_panel_width", "340");
}


// TOTP Cache & Timer
const totpMap = ref<Record<string, TotpResult>>({});
let totpTimer: number | null = null;

// Custom In-App Toast & Confirmation Dialog System
interface ToastItem {
  id: number;
  message: string;
  type: "success" | "error" | "warning" | "info";
}
const toasts = ref<ToastItem[]>([]);
let toastCounter = 0;

function notify(message: string, type: "success" | "error" | "warning" | "info" = "info", duration = 3200) {
  const id = ++toastCounter;
  toasts.value.push({ id, message, type });
  setTimeout(() => {
    toasts.value = toasts.value.filter(t => t.id !== id);
  }, duration);
}

interface ConfirmOptions {
  title?: string;
  message: string;
  confirmText?: string;
  cancelText?: string;
  type?: "danger" | "warning" | "info";
}
const confirmState = ref<{
  visible: boolean;
  title: string;
  message: string;
  confirmText: string;
  cancelText: string;
  type: "danger" | "warning" | "info";
  resolve: (value: boolean) => void;
}>({
  visible: false,
  title: "操作确认",
  message: "",
  confirmText: "确定",
  cancelText: "取消",
  type: "info",
  resolve: () => {}
});

function askConfirm(options: ConfirmOptions | string): Promise<boolean> {
  const opts: ConfirmOptions = typeof options === "string" ? { message: options } : options;
  return new Promise((resolve) => {
    confirmState.value = {
      visible: true,
      title: opts.title || (opts.type === "danger" ? "高危操作确认" : "操作确认"),
      message: opts.message,
      confirmText: opts.confirmText || "确定",
      cancelText: opts.cancelText || "取消",
      type: opts.type || "info",
      resolve: (val: boolean) => {
        confirmState.value.visible = false;
        resolve(val);
      }
    };
  });
}

// UI Helpers
const showPassword = ref(false);
const copiedField = ref<string | null>(null);
const isEditing = ref(false);
const editForm = ref<AccountRecord>({
  id: "",
  email: "",
  password: "",
  group: "",
  status: "待录入",
  recovery_email: "",
  phone: "",
  totp: "",
  verification: "",
  backup_codes: "",
  used_backup_codes: "[]",
  notes: "",
  google_one_plan: "Free",
  google_one_used: "",
  google_one_status: "unknown"
});

const googleOnePlanOptions = [
  "Free",
  "Basic",
  "Standard",
  "Premium",
  "AI Premium",
  "Pro",
  "Ultra"
];

const accountStatusOptions = [
  "待录入",
  "正常",
  "异常",
  "封禁",
  "验证"
];

function getStatusBadgeClass(status?: string) {
  const s = status || "待录入";
  switch (s) {
    case "正常":
      return "px-2 py-0.5 rounded-full text-[10px] font-semibold bg-emerald-50 text-emerald-700 border border-emerald-200/80";
    case "验证":
      return "px-2 py-0.5 rounded-full text-[10px] font-semibold bg-amber-50 text-amber-700 border border-amber-200/80";
    case "异常":
      return "px-2 py-0.5 rounded-full text-[10px] font-semibold bg-rose-50 text-rose-700 border border-rose-200/80";
    case "封禁":
      return "px-2 py-0.5 rounded-full text-[10px] font-bold bg-red-100 text-red-800 border border-red-300";
    case "待录入":
    default:
      return "px-2 py-0.5 rounded-full text-[10px] font-medium bg-gray-100 text-gray-600 border border-gray-200/80";
  }
}

// Windows Hello / Mobile Biometrics state
const helloStatus = ref<{ available: boolean; enrolled: boolean }>({ available: false, enrolled: false });
const isHelloVerifying = ref(false);

const isMobilePlatform = computed(() => {
  if (typeof navigator !== "undefined") {
    return /Android|iPhone|iPad|iPod/i.test(navigator.userAgent);
  }
  return false;
});

async function refreshSecurityStatus() {
  if (isMobilePlatform.value) {
    try {
      const { checkStatus } = await import("@tauri-apps/plugin-biometric");
      const s = await checkStatus();
      const enrolled = !!localStorage.getItem("vault_mobile_biometric_token");
      helloStatus.value = {
        available: s.isAvailable,
        enrolled
      };
    } catch (e) {
      console.warn("Mobile biometric check error:", e);
      const enrolled = !!localStorage.getItem("vault_mobile_biometric_token");
      helloStatus.value = {
        available: true,
        enrolled
      };
    }
  } else {
    try {
      helloStatus.value = await invoke<{ available: boolean; enrolled: boolean }>("check_windows_hello_status");
    } catch (e) {
      console.warn("Check Windows Hello error:", e);
    }
  }
}


// Unlock / Create Vault
async function handleAuth() {
  if (!masterPassword.value) return;
  isSubmitting.value = true;
  unlockError.value = "";
  try {
    if (vaultExists.value) {
      records.value = await invoke<AccountRecord[]>("unlock_vault", { password: masterPassword.value });
    } else {
      await invoke("create_vault", { password: masterPassword.value });
      records.value = [];
      vaultExists.value = true;
    }
    isUnlocked.value = true;
    if (autoSelectFirst.value && records.value.length > 0) {
      selectedId.value = records.value[0].id;
    } else {
      selectedId.value = null;
    }
    startTotpTicker();
    await loadWebDavConfig();
    if (webdavForm.value.auto_sync) {
      triggerSmartSync(true);
    }
  } catch (err: any) {
    unlockError.value = typeof err === "string" ? err : (err?.message || "解锁失败");
  } finally {
    isSubmitting.value = false;
  }
}

async function handleHelloUnlock() {
  isHelloVerifying.value = true;
  unlockError.value = "";
  try {
    if (isMobilePlatform.value) {
      const cred = localStorage.getItem("vault_mobile_biometric_token");
      if (!cred) {
        throw new Error("手机端尚未绑定生物识别，请使用主密码登录");
      }
      const { biometricCipher } = await import("@tauri-apps/plugin-biometric");
      const res = await biometricCipher("验证指纹、面容或锁屏密码以解锁保险库", {
        dataToDecrypt: cred,
        title: "安全验证解锁",
        subtitle: "请验证指纹、面容或锁屏密码",
        allowDeviceCredential: true
      });
      if (!res || !res.data) {
        throw new Error("生物识别解密失败");
      }
      const pwd = res.data;
      records.value = await invoke<AccountRecord[]>("unlock_vault", { password: pwd });
      masterPassword.value = pwd;
      isUnlocked.value = true;
      if (autoSelectFirst.value && records.value.length > 0) {
        selectedId.value = records.value[0].id;
      } else {
        selectedId.value = null;
      }
      startTotpTicker();
      await loadWebDavConfig();
      if (webdavForm.value.auto_sync) {
        triggerSmartSync(true);
      }
    } else {
      records.value = await invoke<AccountRecord[]>("unlock_vault_with_hello");
      isUnlocked.value = true;
      if (autoSelectFirst.value && records.value.length > 0) {
        selectedId.value = records.value[0].id;
      } else {
        selectedId.value = null;
      }
      startTotpTicker();
      await loadWebDavConfig();
      if (webdavForm.value.auto_sync) {
        triggerSmartSync(true);
      }
    }
  } catch (err: any) {
    const defaultMsg = isMobilePlatform.value ? "手机安全验证未通过" : "Windows Hello 验证未通过";
    unlockError.value = typeof err === "string" ? err : (err?.message || defaultMsg);
  } finally {
    isHelloVerifying.value = false;
  }
}

async function toggleWindowsHello() {
  try {
    if (isMobilePlatform.value) {
      if (helloStatus.value.enrolled) {
        localStorage.removeItem("vault_mobile_biometric_token");
        helloStatus.value.enrolled = false;
        notify("已停用手机生物识别 / 屏幕锁免密解锁", "info");
      } else {
        if (!masterPassword.value) {
          notify("未获取到当前主密码，请重新使用主密码解锁后再绑定", "warning");
          return;
        }
        const { biometricCipher } = await import("@tauri-apps/plugin-biometric");
        const res = await biometricCipher("验证手机指纹、面容或屏幕锁以绑定快速解锁", {
          dataToEncrypt: masterPassword.value,
          title: "绑定安全验证",
          subtitle: "验证指纹、面容或锁屏密码以绑定快速登录",
          allowDeviceCredential: true
        });
        if (res && res.data) {
          localStorage.setItem("vault_mobile_biometric_token", res.data);
          helloStatus.value.enrolled = true;
          notify("手机安全验证已成功绑定！下次可直接通过指纹/面容/屏幕锁一键解锁", "success", 4000);
        }
      }
    } else {
      if (helloStatus.value.enrolled) {
        await invoke("disable_windows_hello");
        helloStatus.value.enrolled = false;
        notify("已停用 Windows Hello / PIN 快速免密解锁", "info");
      } else {
        await invoke("enable_windows_hello");
        helloStatus.value.enrolled = true;
        notify("Windows Hello / PIN 快速解锁已成功绑定！下次可直接指纹/面容/PIN一键解锁", "success", 4000);
      }
    }
  } catch (err: any) {
    const errorMsg = typeof err === "string" ? err : (err?.message || "操作取消或验证未通过");
    notify((isMobilePlatform.value ? "绑定安全验证失败: " : "设置 Windows Hello 失败: ") + errorMsg, "error");
  }
}

async function lockVault() {
  await invoke("lock_vault");
  isUnlocked.value = false;
  masterPassword.value = "";
  records.value = [];
  selectedId.value = null;
  mobileView.value = "list";
  showMobileMoreMenu.value = false;
  showMobileSearch.value = false;
  if (totpTimer) clearInterval(totpTimer);
  await refreshSecurityStatus();
}

// TOTP Periodic Updates
function startTotpTicker() {
  updateAllTotp();
  if (totpTimer) clearInterval(totpTimer);
  totpTimer = window.setInterval(updateAllTotp, 1000);
}

async function updateAllTotp() {
  for (const r of records.value) {
    if (r.totp && r.totp.trim()) {
      try {
        const res = await invoke<TotpResult>("get_totp", { secret: r.totp });
        totpMap.value[r.id] = res;
      } catch (e) {
        // invalid secret
      }
    }
  }
}

// Computed Filtering
const selectedRecord = computed(() => {
  return records.value.find((r) => r.id === selectedId.value) || null;
});

const groups = computed(() => {
  const custom = Array.from(new Set(records.value.map((r) => (r.group || "").trim()).filter(Boolean))).sort();
  return [
    { key: null, label: "全部账号", count: records.value.length },
    { key: "one_active", label: "One 有效", count: records.value.filter((r) => r.google_one_status === "active").length },
    { key: "one_expired", label: "One 已失效", count: records.value.filter((r) => r.google_one_status === "expired").length },
    { key: "one_inactive", label: "One 非会员", count: records.value.filter((r) => r.google_one_status === "inactive").length },
    { key: "one_unknown", label: "One 未验证", count: records.value.filter((r) => !r.google_one_status || r.google_one_status === "unknown").length },
    { key: "", label: "未分组", count: records.value.filter((r) => !(r.group || "").trim()).length },
    ...custom.map((name) => ({
      key: name,
      label: name,
      count: records.value.filter((r) => (r.group || "").trim() === name).length
    }))
  ];
});

const filteredRecords = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  return records.value.filter((r) => {
    // Group filter
    if (currentGroup.value === "one_active" && r.google_one_status !== "active") return false;
    if (currentGroup.value === "one_expired" && r.google_one_status !== "expired") return false;
    if (currentGroup.value === "one_inactive" && r.google_one_status !== "inactive") return false;
    if (currentGroup.value === "one_unknown" && r.google_one_status && r.google_one_status !== "unknown") return false;
    if (currentGroup.value === "" && (r.group || "").trim() !== "") return false;
    if (currentGroup.value && !currentGroup.value.startsWith("one_") && currentGroup.value !== "" && (r.group || "").trim() !== currentGroup.value) {
      return false;
    }

    // Search query
    if (q) {
      const match = [r.email, r.group, r.notes, r.recovery_email, r.status, r.google_one_plan].join(" ").toLowerCase();
      if (!match.includes(q)) return false;
    }
    return true;
  });
});

// Copy Helper
async function copyText(text: string, fieldKey: string, toastMsg = "已成功复制到剪贴板") {
  if (!text) return;
  await navigator.clipboard.writeText(text);
  copiedField.value = fieldKey;
  notify(toastMsg, "success", 1800);
  setTimeout(() => {
    if (copiedField.value === fieldKey) copiedField.value = null;
  }, 2000);
}

// Edit / Add Actions
function openAddModal() {
  editForm.value = {
    id: "",
    email: "",
    password: "",
    group: "",
    status: "待录入",
    recovery_email: "",
    phone: "",
    totp: "",
    verification: "",
    backup_codes: "",
    used_backup_codes: "[]",
    notes: "",
    google_one_plan: "Free",
    google_one_used: "",
    google_one_status: "unknown"
  };
  isEditing.value = true;
}

function openEditModal(record: AccountRecord) {
  editForm.value = JSON.parse(JSON.stringify(record));
  if (!editForm.value.google_one_plan) {
    editForm.value.google_one_plan = "Free";
  }
  if (!editForm.value.status) {
    editForm.value.status = "待录入";
  }
  isEditing.value = true;
}

const isSavingRecord = ref(false);

async function handleSaveRecord() {
  if (!editForm.value.email.trim() || isSavingRecord.value) return;
  isSavingRecord.value = true;
  try {
    const saved = await invoke<AccountRecord>("save_record", { record: editForm.value });
    const idx = records.value.findIndex((r) => r.id === saved.id);
    if (idx >= 0) {
      records.value[idx] = saved;
    } else {
      records.value.unshift(saved);
    }
    selectedId.value = saved.id;
    isEditing.value = false;
    updateAllTotp();
    notify("账号信息已成功加密保存", "success");
    triggerDebouncedSync();
  } catch (err: any) {
    notify("保存失败: " + err, "error");
  } finally {
    isSavingRecord.value = false;
  }
}

async function handleDeleteRecord(id: string) {
  const confirmed = await askConfirm({
    title: "彻底删除账号记录",
    message: "确定要从本地加密安全库中删除该账号吗？此操作不可逆。",
    type: "danger",
    confirmText: "确认删除",
    cancelText: "取消"
  });
  if (!confirmed) return;
  try {
    await invoke("delete_record", { id });
    records.value = records.value.filter((r) => r.id !== id);
    if (selectedId.value === id) {
      selectedId.value = records.value[0]?.id || null;
      mobileView.value = "list";
    }
    notify("账号记录已安全删除", "info");
    triggerDebouncedSync();
  } catch (err: any) {
    notify("删除失败: " + err, "error");
  }
}

async function generatePassword() {
  try {
    const pwd = await invoke<string>("generate_pwd", { length: 24, symbols: true });
    editForm.value.password = pwd;
  } catch (e) {
    // fallback
  }
}

// In-App Isolated Browser (Shape A)
const isOpeningBrowser = ref(false);
const showCustomUrlModal = ref(false);
const customUrlInput = ref("https://");

const servicePresets = [
  { name: "Google 账号", url: "https://myaccount.google.com" },
  { name: "Gmail 邮箱", url: "https://mail.google.com" },
  { name: "云端硬盘", url: "https://drive.google.com" },
  { name: "YouTube", url: "https://www.youtube.com" },
  { name: "安全设置与 2FA", url: "https://myaccount.google.com/security" },
  { name: "Google One 存储", url: "https://one.google.com/storage" },
  { name: "付款中心与账单", url: "https://payments.google.com/gp/w/home/settings" },
  { name: "服务条款与归属地", url: "https://policies.google.com/terms" },
];

async function openInApp(url: string) {
  if (!selectedRecord.value) return;
  isOpeningBrowser.value = true;
  try {
    await invoke("open_in_app_browser", {
      email: selectedRecord.value.email,
      password: selectedRecord.value.password || "",
      totpSecret: selectedRecord.value.totp || "",
      recoveryEmail: selectedRecord.value.recovery_email || "",
      url
    });
  } catch (err: any) {
    notify("应用内浏览器启动异常，正在通过系统浏览器打开", "warning");
    try {
      await invoke("open_external_url", { url });
    } catch (_) {}
  } finally {
    isOpeningBrowser.value = false;
  }
}

function openCustomUrlModal() {
  customUrlInput.value = "https://";
  showCustomUrlModal.value = true;
}

function handleOpenCustomUrl() {
  let url = customUrlInput.value.trim();
  if (!url) return;
  if (!url.startsWith("http://") && !url.startsWith("https://")) {
    url = "https://" + url;
  }
  showCustomUrlModal.value = false;
  openInApp(url);
}

// ==================== 1. BACKUP CODES MANAGEMENT ====================
const showBackupCodesModal = ref(false);
const backupCodesRawInput = ref("");
const isEditingBackupCodesRaw = ref(false);

const parsedBackupCodesList = computed(() => {
  if (!selectedRecord.value?.backup_codes) return [];
  const raw = selectedRecord.value.backup_codes.trim();
  const matches = raw.match(/\b\d{4}\s*\d{4}\b|\b\d{8}\b/g) || raw.split(/[\r\n,;\t]+/).map(s => s.trim()).filter(Boolean);
  return matches.map(c => c.replace(/\s+/g, ""));
});

const usedBackupCodesSet = computed(() => {
  try {
    const arr = JSON.parse(selectedRecord.value?.used_backup_codes || "[]");
    return new Set<string>(Array.isArray(arr) ? arr : []);
  } catch {
    return new Set<string>();
  }
});

const unusedBackupCodesCount = computed(() => {
  const all = parsedBackupCodesList.value;
  const used = usedBackupCodesSet.value;
  return all.filter(c => !used.has(c)).length;
});

async function toggleBackupCode(code: string) {
  if (!selectedRecord.value) return;
  const currentSet = new Set(usedBackupCodesSet.value);
  if (currentSet.has(code)) {
    currentSet.delete(code);
  } else {
    currentSet.add(code);
  }
  selectedRecord.value.used_backup_codes = JSON.stringify(Array.from(currentSet));
  await invoke("save_record", { record: selectedRecord.value });
  triggerDebouncedSync();
}

async function copyAndConsumeNextBackupCode() {
  if (!selectedRecord.value) return;
  const all = parsedBackupCodesList.value;
  const used = new Set(usedBackupCodesSet.value);
  const next = all.find(c => !used.has(c));
  if (!next) {
    notify("当前账号所有备用验证码已全部标记为已使用！请在 Google 安全设置中生成新代码。", "warning");
    return;
  }
  await navigator.clipboard.writeText(next);
  used.add(next);
  selectedRecord.value.used_backup_codes = JSON.stringify(Array.from(used));
  await invoke("save_record", { record: selectedRecord.value });
  copiedField.value = "backup_code_consumed";
  notify(`⚡ 已复制未用码 [${next}] 并自动完成核销`, "success", 3000);
  triggerDebouncedSync();
  setTimeout(() => {
    if (copiedField.value === "backup_code_consumed") copiedField.value = null;
  }, 2500);
}

function openBackupCodesModal() {
  if (!selectedRecord.value) return;
  backupCodesRawInput.value = parsedBackupCodesList.value.join("\n");
  isEditingBackupCodesRaw.value = false;
  showBackupCodesModal.value = true;
}

async function saveRawBackupCodes() {
  if (!selectedRecord.value) return;
  const codes = backupCodesRawInput.value.split(/[\r\n,;\t]+/).map(s => s.replace(/\s+/g, "").trim()).filter(Boolean);
  selectedRecord.value.backup_codes = codes.join("\n");
  const codeSet = new Set(codes);
  const currentUsed = Array.from(usedBackupCodesSet.value).filter(c => codeSet.has(c));
  selectedRecord.value.used_backup_codes = JSON.stringify(currentUsed);
  await invoke("save_record", { record: selectedRecord.value });
  isEditingBackupCodesRaw.value = false;
  notify(`成功更新 ${codes.length} 个备用验证码`, "success");
  triggerDebouncedSync();
}

async function resetAllUsedCodes() {
  if (!selectedRecord.value) return;
  const confirmed = await askConfirm({
    title: "重置备用验证码状态",
    message: "确定要将所有备用验证码恢复为「未使用」吗？",
    type: "warning",
    confirmText: "确认重置",
    cancelText: "取消"
  });
  if (!confirmed) return;
  selectedRecord.value.used_backup_codes = "[]";
  await invoke("save_record", { record: selectedRecord.value });
  notify("所有备用验证码状态已全部重置为未用", "info");
  triggerDebouncedSync();
}

// ==================== 2. BATCH IMPORT & SMART PARSER ====================
const showBatchImportModal = ref(false);
const batchRawText = ref("");
const batchAssignGroup = ref("");
const batchParsedRecords = ref<AccountRecord[]>([]);

function openBatchImportModal() {
  batchRawText.value = "";
  batchAssignGroup.value = "";
  batchParsedRecords.value = [];
  showBatchImportModal.value = true;
}

function parseSmartLine(line: string): AccountRecord | null {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith("#")) return null;

  let parts: string[] = [];
  if (trimmed.includes("----")) {
    parts = trimmed.split("----");
  } else if (trimmed.includes("\t")) {
    parts = trimmed.split("\t");
  } else if (trimmed.includes("|")) {
    parts = trimmed.split("|");
  } else if (trimmed.includes(",")) {
    parts = trimmed.split(",");
  } else {
    parts = trimmed.split(/\s+/);
  }
  parts = parts.map(p => p.trim()).filter(Boolean);
  if (parts.length === 0) return null;

  let email = "";
  let password = "";
  let recoveryEmail = "";
  let totp = "";
  const backupCodes: string[] = [];
  const notes: string[] = [];

  const emailRegex = /^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$/;
  const base32Regex = /^[A-Z2-7]{16,32}$/i;

  for (const part of parts) {
    if (part.startsWith("otpauth://")) {
      totp = part;
      continue;
    }
    if (emailRegex.test(part)) {
      if (!email) {
        email = part;
      } else if (!recoveryEmail) {
        recoveryEmail = part;
      }
      continue;
    }
    if (base32Regex.test(part) && part.length >= 16 && !totp) {
      totp = part;
      continue;
    }
    if (/^\d{8}$/.test(part.replace(/\s+/g, ""))) {
      backupCodes.push(part.replace(/\s+/g, ""));
      continue;
    }
    if (!password) {
      password = part;
      continue;
    }
    notes.push(part);
  }

  if (!email) return null;

  return {
    id: "",
    email,
    password,
    group: batchAssignGroup.value.trim(),
    status: "待录入",
    recovery_email: recoveryEmail,
    phone: "",
    totp,
    verification: "",
    backup_codes: backupCodes.join("\n"),
    used_backup_codes: "[]",
    notes: notes.join(" "),
    google_one_plan: "Free",
    google_one_used: "",
    google_one_status: "unknown"
  };
}

function handleBatchParse() {
  const lines = batchRawText.value.split(/\r?\n/);
  const result: AccountRecord[] = [];
  for (const line of lines) {
    const parsed = parseSmartLine(line);
    if (parsed) {
      if (batchAssignGroup.value.trim()) {
        parsed.group = batchAssignGroup.value.trim();
      }
      result.push(parsed);
    }
  }
  batchParsedRecords.value = result;
}

async function confirmBatchImport() {
  if (batchParsedRecords.value.length === 0) return;
  try {
    const count = await invoke<number>("batch_import_records", {
      newRecords: batchParsedRecords.value
    });
    records.value = await invoke<AccountRecord[]>("get_records");
    showBatchImportModal.value = false;
    notify(`🎉 成功批量导入/合并了 ${count} 个账号记录！`, "success", 4000);
    if (autoSelectFirst.value && !selectedId.value && records.value.length > 0) {
      selectedId.value = records.value[0].id;
    }
    updateAllTotp();
    triggerDebouncedSync();
  } catch (err: any) {
    notify("批量导入失败: " + err, "error");
  }
}

// ==================== 3. WEBDAV CLOUD SYNC ====================
interface SmartSyncResult {
  synced: boolean;
  message: string;
  records_updated: boolean;
  records_count: number;
  last_sync_time: number;
}

type SyncState = "idle" | "syncing" | "success" | "error";

const webdavForm = ref({
  url: "",
  username: "",
  password: "",
  auto_sync: true,
  last_sync_time: null as number | null,
  last_etag: null as string | null
});
const webdavStatus = ref("");
const isWebDavSyncing = ref(false);
const syncState = ref<SyncState>("idle");
const syncTooltip = ref<string>("WebDAV 云端同步就绪");

const lastSyncTimeFormatted = computed(() => {
  if (!webdavForm.value.last_sync_time) return "从未同步";
  const d = new Date(webdavForm.value.last_sync_time);
  const now = new Date();
  const diffSec = Math.floor((now.getTime() - d.getTime()) / 1000);
  if (diffSec < 60) return "刚刚";
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)} 分钟前`;
  const month = d.getMonth() + 1;
  const day = d.getDate();
  const hours = String(d.getHours()).padStart(2, "0");
  const mins = String(d.getMinutes()).padStart(2, "0");
  return `${month}/${day} ${hours}:${mins}`;
});

async function loadWebDavConfig() {
  try {
    const saved = await invoke<any>("load_webdav_config");
    if (saved && (saved.url || saved.username || saved.password)) {
      webdavForm.value = {
        url: saved.url || "",
        username: saved.username || "",
        password: saved.password || "",
        auto_sync: saved.auto_sync !== false,
        last_sync_time: saved.last_sync_time || null,
        last_etag: saved.last_etag || null
      };
      try {
        localStorage.setItem("gvault_webdav_config", JSON.stringify(webdavForm.value));
      } catch (e) {}
      return;
    }
  } catch (e) {
    console.warn("load_webdav_config error:", e);
  }

  // Fallback to localStorage cache
  try {
    const localStr = localStorage.getItem("gvault_webdav_config");
    if (localStr) {
      const parsed = JSON.parse(localStr);
      if (parsed && (parsed.url || parsed.username || parsed.password)) {
        webdavForm.value = {
          url: parsed.url || "",
          username: parsed.username || "",
          password: parsed.password || "",
          auto_sync: parsed.auto_sync !== false,
          last_sync_time: parsed.last_sync_time || null,
          last_etag: parsed.last_etag || null
        };
      }
    }
  } catch (e) {}
}

async function handleSaveWebDavConfig(notifyUser = true) {
  try {
    await invoke("save_webdav_config", { config: webdavForm.value });
    try {
      localStorage.setItem("gvault_webdav_config", JSON.stringify(webdavForm.value));
    } catch (e) {}
    if (notifyUser) {
      webdavStatus.value = "✅ 配置已成功保存到本地安全存储";
      notify("WebDAV 配置已保存", "success", 2500);
    }
  } catch (err: any) {
    webdavStatus.value = "❌ 保存失败: " + err;
    if (notifyUser) notify("保存 WebDAV 配置失败: " + err, "error");
  }
}

let isSyncingSmart = false;
let debouncedSyncTimer: any = null;

async function triggerSmartSync(silent = false) {
  if (isSyncingSmart) return;
  if (!webdavForm.value.url.trim() || !webdavForm.value.username.trim() || !webdavForm.value.password.trim()) {
    syncState.value = "idle";
    syncTooltip.value = "未配置 WebDAV 云端同步";
    if (!silent) {
      notify("请先在「设置 -> WebDAV 云端同步」中配置服务器地址与密码", "warning");
    }
    return;
  }
  if (silent && !webdavForm.value.auto_sync) {
    return;
  }

  isSyncingSmart = true;
  isWebDavSyncing.value = true;
  syncState.value = "syncing";
  syncTooltip.value = "正在智能检测并同步云端数据...";

  try {
    const res = await invoke<SmartSyncResult>("sync_webdav_smart");
    if (res.synced) {
      syncState.value = "success";
      webdavForm.value.last_sync_time = res.last_sync_time;
      syncTooltip.value = res.message;
      if (res.records_updated) {
        records.value = await invoke<AccountRecord[]>("get_records");
        updateAllTotp();
        if (!silent) {
          notify(`🎉 ${res.message} (当前共 ${res.records_count} 个有效账号)`, "success", 3000);
        }
      } else if (!silent) {
        notify(res.message, "success", 2500);
      }
    } else {
      syncState.value = "idle";
      syncTooltip.value = res.message;
      if (!silent) {
        notify(res.message, "info", 2500);
      }
    }
  } catch (err: any) {
    console.warn("Smart sync error:", err);
    syncState.value = "error";
    const errStr = typeof err === "string" ? err : (err?.message || "网络请求异常");
    syncTooltip.value = `同步失败: ${errStr}`;
    if (!silent) {
      notify(`云同步失败: ${errStr}`, "error", 4000);
    }
  } finally {
    isSyncingSmart = false;
    isWebDavSyncing.value = false;
    if (syncState.value === "success") {
      setTimeout(() => {
        if (syncState.value === "success") {
          syncState.value = "idle";
          syncTooltip.value = `云端同步正常 (上次: ${lastSyncTimeFormatted.value})`;
        }
      }, 3500);
    }
  }
}

function triggerDebouncedSync() {
  if (!webdavForm.value.auto_sync || !webdavForm.value.url.trim()) return;
  if (debouncedSyncTimer) clearTimeout(debouncedSyncTimer);
  debouncedSyncTimer = setTimeout(() => {
    triggerSmartSync(true);
  }, 1500);
}

// ==================== SETTINGS & GITHUB UPDATE ====================
const showSettingsModal = ref(false);
const settingsTab = ref<"security" | "webdav" | "backups" | "updates" | "about">("security");

interface ReleaseAsset {
  name: string;
  download_url: string;
  size: number;
  asset_type: string;
}

interface UpdateInfo {
  current_version: string;
  latest_version: string;
  has_update: boolean;
  release_name: string;
  release_notes: string;
  published_at: string;
  release_url: string;
  download_url: string | null;
  setup_url: string | null;
  portable_url: string | null;
  apk_url: string | null;
  current_install_type: string;
  assets: ReleaseAsset[];
}
type UpdateMirror = "gh-proxy" | "ghproxy" | "official";

const customRepo = ref("ifnor/GoogleAccountVault");
const updateMirror = ref<UpdateMirror>(
  (localStorage.getItem("gvault_update_mirror") as UpdateMirror) || "gh-proxy"
);

function setUpdateMirror(mirror: UpdateMirror) {
  updateMirror.value = mirror;
  try {
    localStorage.setItem("gvault_update_mirror", mirror);
  } catch (e) {}
}
const isCheckingUpdate = ref(false);
const updateResult = ref<UpdateInfo | null>(null);
const updateError = ref("");

const currentInstallTypeName = computed(() => {
  if (isMobilePlatform.value || updateResult.value?.current_install_type === "android") {
    return "Android 手机版";
  }
  if (updateResult.value?.current_install_type === "portable") {
    return "Windows 绿色便携版";
  }
  return "Windows 安装版";
});

interface ParsedChangelogItem {
  type: string;
  typeLabel: string;
  typeColor: string;
  scope?: string;
  scopeLabel?: string;
  text: string;
}

interface ParsedReleaseNotes {
  title: string;
  items: ParsedChangelogItem[];
  rawText?: string;
}

const parsedReleaseNotes = computed<ParsedReleaseNotes | null>(() => {
  const raw = updateResult.value?.release_notes?.trim();
  if (!raw) return null;

  const lines = raw.split(/\r?\n/);
  let title = "🚀 本次更新内容";
  const items: ParsedChangelogItem[] = [];

  for (const line of lines) {
    const trimmed = line.trim();
    if (!trimmed) continue;

    // Remove markdown heading tokens like ### or ##
    const headerMatch = trimmed.match(/^#{1,6}\s*(.*)$/);
    if (headerMatch) {
      const headerText = headerMatch[1].trim();
      if (headerText) {
        title = headerText;
      }
      continue;
    }

    // Strip leading bullet markers like "* ", "- ", "• ", "+ "
    const content = trimmed.replace(/^[\*\-\•\+]\s+/, "").trim();
    if (!content) continue;

    // Conventional Commits format: feat(scope): message or fix: message
    const commitMatch = content.match(/^([a-zA-Z]+)(?:\(([^)]+)\))?\s*[:：]\s*(.*)$/);

    if (commitMatch) {
      const rawType = commitMatch[1].toLowerCase();
      const rawScope = commitMatch[2]?.trim() || "";
      const message = commitMatch[3].trim();

      let typeLabel = "优化";
      let typeColor = "bg-blue-50 text-blue-700 border-blue-200";

      if (rawType === "feat") {
        typeLabel = "新增";
        typeColor = "bg-emerald-50 text-emerald-700 border-emerald-200";
      } else if (rawType === "fix") {
        typeLabel = "修复";
        typeColor = "bg-amber-50 text-amber-700 border-amber-200";
      } else if (rawType === "perf") {
        typeLabel = "性能";
        typeColor = "bg-teal-50 text-teal-700 border-teal-200";
      } else if (rawType === "refactor") {
        typeLabel = "重构";
        typeColor = "bg-purple-50 text-purple-700 border-purple-200";
      } else if (rawType === "chore") {
        typeLabel = "工程";
        typeColor = "bg-slate-50 text-slate-700 border-slate-200";
      } else if (rawType === "docs") {
        typeLabel = "文档";
        typeColor = "bg-sky-50 text-sky-700 border-sky-200";
      } else if (rawType === "style") {
        typeLabel = "界面";
        typeColor = "bg-pink-50 text-pink-700 border-pink-200";
      }

      let scopeLabel = rawScope;
      const lowerScope = rawScope.toLowerCase();
      if (lowerScope === "android") scopeLabel = "Android";
      else if (lowerScope === "desktop" || lowerScope === "windows") scopeLabel = "桌面端";
      else if (lowerScope === "ci" || lowerScope === "actions") scopeLabel = "CI构建";
      else if (lowerScope === "update" || lowerScope === "updater") scopeLabel = "自动更新";
      else if (lowerScope === "ui") scopeLabel = "UI界面";
      else if (lowerScope === "vault" || lowerScope === "security") scopeLabel = "安全库";
      else if (lowerScope === "webdav" || lowerScope === "sync") scopeLabel = "云端同步";

      items.push({
        type: rawType,
        typeLabel,
        typeColor,
        scope: rawScope,
        scopeLabel: scopeLabel || undefined,
        text: message || content
      });
    } else {
      items.push({
        type: "other",
        typeLabel: "详情",
        typeColor: "bg-slate-50 text-slate-600 border-slate-200",
        text: content
      });
    }
  }

  if (items.length === 0) {
    return {
      title,
      items: [],
      rawText: raw
    };
  }

  return {
    title,
    items
  };
});
const isDownloadingUpdate = ref(false);
const downloadProgress = ref(0);
const downloadStatusText = ref("");
let unlistenUpdateProgress: UnlistenFn | null = null;

async function openSettingsModal(tab: "security" | "webdav" | "backups" | "updates" | "about" = "security") {
  settingsTab.value = tab;
  showSettingsModal.value = true;
  webdavStatus.value = "";
  await loadWebDavConfig();
  if (tab === "backups") {
    await loadBackupsList();
  }
}

async function checkForUpdates() {
  isCheckingUpdate.value = true;
  updateError.value = "";
  updateResult.value = null;
  try {
    const mirrorParam =
      updateMirror.value === "gh-proxy"
        ? "https://gh-proxy.com"
        : updateMirror.value === "ghproxy"
        ? "https://ghproxy.net"
        : "official";

    const res = await invoke<UpdateInfo>("check_github_update", {
      repo: customRepo.value.trim() || undefined,
      mirror: mirrorParam
    });
    updateResult.value = res;
    if (res.has_update) {
      notify(`✨ 发现新版本 ${res.latest_version}！可在设置中查看更新`, "info", 5000);
    } else {
      notify(`当前已是最新版本 (${res.current_version})`, "success", 3000);
    }
  } catch (err: any) {
    updateError.value = typeof err === "string" ? err : (err?.message || "检查更新失败");
    notify("检查更新失败: " + updateError.value, "error");
  } finally {
    isCheckingUpdate.value = false;
  }
}

async function openExternalUrl(url?: string | null) {
  if (!url) return;
  try {
    await openUrl(url);
    return;
  } catch (e1) {
    try {
      await invoke("open_external_url", { url });
      return;
    } catch (e2) {
      window.open(url, "_blank");
    }
  }
}

async function startAutoUpdate(targetUrl?: string) {
  const url =
    targetUrl ||
    updateResult.value?.download_url ||
    (isMobilePlatform.value ? updateResult.value?.apk_url : updateResult.value?.setup_url);
  if (!url) {
    notify("未找到有效下载地址，请尝试浏览器备用下载", "error");
    return;
  }

  isDownloadingUpdate.value = true;
  downloadProgress.value = 0;
  downloadStatusText.value = "正在连接下载服务器...";

  try {
    await invoke("start_auto_update", { url });
  } catch (err: any) {
    isDownloadingUpdate.value = false;
    const msg = typeof err === "string" ? err : (err?.message || "自动更新失败");
    notify("自动更新异常: " + msg, "error", 6000);
    downloadStatusText.value = "更新异常: " + msg;
  }
}

async function handleTestWebDav() {
  if (!webdavForm.value.url.trim()) return;
  await handleSaveWebDavConfig(false);
  isWebDavSyncing.value = true;
  webdavStatus.value = "正在连接 WebDAV 服务器...";
  try {
    const msg = await invoke<string>("test_webdav_connection", { config: webdavForm.value });
    webdavStatus.value = "✅ " + msg;
  } catch (err: any) {
    webdavStatus.value = "❌ " + err;
  } finally {
    isWebDavSyncing.value = false;
  }
}

async function handleUploadWebDav() {
  isWebDavSyncing.value = true;
  webdavStatus.value = "正在上传加密保险库至云端...";
  try {
    await handleSaveWebDavConfig(false);
    await invoke("upload_webdav_vault", { config: webdavForm.value });
    webdavStatus.value = "✅ 本地保险库已成功同步上传至云端（端到端强加密密文）！";
  } catch (err: any) {
    webdavStatus.value = "❌ 上传失败: " + err;
  } finally {
    isWebDavSyncing.value = false;
  }
}

async function handleDownloadWebDav() {
  const confirmed = await askConfirm({
    title: "云端恢复确认",
    message: "从云端下载将覆盖本地现有保险库（系统会自动先为当前本地数据创建安全快照备份）。确定继续吗？",
    type: "warning",
    confirmText: "确认恢复覆盖",
    cancelText: "取消"
  });
  if (!confirmed) return;
  isWebDavSyncing.value = true;
  webdavStatus.value = "正在下载并解密验证云端保险库...";
  try {
    await handleSaveWebDavConfig(false);
    const remote = await invoke<AccountRecord[]>("download_webdav_vault", { config: webdavForm.value });
    records.value = remote;
    if (autoSelectFirst.value && remote.length > 0) selectedId.value = remote[0].id; else selectedId.value = null;
    updateAllTotp();
    webdavStatus.value = `✅ 云端恢复成功！共同步载入 ${remote.length} 个账号。`;
    notify(`云端恢复成功！已同步载入 ${remote.length} 个账号`, "success");
  } catch (err: any) {
    webdavStatus.value = "❌ 恢复失败: " + err;
    notify("云端恢复失败: " + err, "error");
  } finally {
    isWebDavSyncing.value = false;
  }
}

// ==================== 4. BACKUPS & EXPORT ====================
const backupTab = ref<"backups" | "export">("backups");
const backupList = ref<Array<{ filename: string; timestamp: string; size_bytes: number }>>([]);
const isLoadingBackups = ref(false);

async function loadBackupsList() {
  isLoadingBackups.value = true;
  try {
    backupList.value = await invoke("list_vault_backups");
  } catch (e) {
    backupList.value = [];
  } finally {
    isLoadingBackups.value = false;
  }
}

async function handleRestoreBackup(filename: string) {
  const confirmed = await askConfirm({
    title: "回滚历史快照",
    message: `确定要回滚到历史版本 [${filename}] 吗？当前最新数据将自动创建新的安全快照备份。`,
    type: "warning",
    confirmText: "确认回滚",
    cancelText: "取消"
  });
  if (!confirmed) return;
  try {
    const restored = await invoke<AccountRecord[]>("restore_vault_backup", { filename });
    records.value = restored;
    if (autoSelectFirst.value && restored.length > 0) selectedId.value = restored[0].id; else selectedId.value = null;
    updateAllTotp();
    await loadBackupsList();
    notify(`🎉 成功回滚到历史版本！已载入 ${restored.length} 个账号记录。`, "success", 4000);
  } catch (err: any) {
    notify("回滚失败: " + err, "error");
  }
}

async function handleExportGvault() {
  try {
    const encryptedContent = await invoke<string>("export_encrypted_vault_string");
    const blob = new Blob([encryptedContent], { type: "application/json;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    const now = new Date();
    const dateStr = now.toISOString().slice(0, 10).replace(/-/g, "");
    link.download = `GoogleAccountVault_${dateStr}.gvault`;
    link.click();
    URL.revokeObjectURL(url);
    notify("🛡️ 密文保险库文件 (.gvault) 已成功导出并保存", "success", 3000);
  } catch (err: any) {
    notify("导出密文保险库失败: " + err, "error");
  }
}

function triggerImportGvaultFile() {
  const input = document.createElement("input");
  input.type = "file";
  input.accept = ".gvault,.json";
  input.onchange = async (e: any) => {
    const file = e.target?.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      await handleImportGvaultBlob(text, file.name);
    } catch (err: any) {
      notify("读取文件失败: " + err, "error");
    }
  };
  input.click();
}

async function handleImportGvaultBlob(blob: string, fileName: string) {
  const confirmed = await askConfirm({
    title: "确认导入密文保险库？",
    message: `确定要从密文文件 [${fileName}] 导入并合并账号吗？\n将使用当前主密码进行 AES-256-GCM 解密，如有重复邮箱将自动更新。`,
    type: "info",
    confirmText: "确认导入",
    cancelText: "取消"
  });
  if (!confirmed) return;

  try {
    const count = await invoke<number>("import_encrypted_vault_blob", { blob });
    records.value = await invoke<AccountRecord[]>("get_records");
    updateAllTotp();
    notify(`🎉 成功从密文文件导入并更新了 ${count} 条账号记录！`, "success", 4000);
    triggerDebouncedSync();
  } catch (err: any) {
    notify("导入密文保险库失败（请核对主密码是否一致）: " + err, "error", 6000);
  }
}

async function handleExportCsv() {
  try {
    const csvContent = await invoke<string>("export_records_csv_string");
    const blob = new Blob(["\uFEFF" + csvContent], { type: "text/csv;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `GoogleAccountVault_Export_${new Date().toISOString().slice(0, 10)}.csv`;
    link.click();
    URL.revokeObjectURL(url);
    notify("CSV 表格已成功导出并保存", "success");
  } catch (err: any) {
    notify("导出失败: " + err, "error");
  }
}

function handleExportJson() {
  try {
    const jsonStr = JSON.stringify(records.value, null, 2);
    const blob = new Blob([jsonStr], { type: "application/json;charset=utf-8;" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `GoogleAccountVault_Export_${new Date().toISOString().slice(0, 10)}.json`;
    link.click();
    URL.revokeObjectURL(url);
    notify("JSON 数据已成功导出并保存", "success");
  } catch (err: any) {
    notify("导出失败: " + err, "error");
  }
}

// ==================== 5. PASSWORD GENERATOR ====================
const showPasswordGenModal = ref(false);
const pwdGenForm = ref({
  length: 20,
  uppercase: true,
  lowercase: true,
  numbers: true,
  symbols: true,
  exclude_ambiguous: false
});
const generatedPassword = ref("");

async function generateNewPassword() {
  try {
    generatedPassword.value = await invoke<string>("generate_custom_pwd", {
      options: pwdGenForm.value
    });
  } catch (err: any) {
    notify("生成密码失败: " + err, "error");
  }
}

function openPasswordGenModal() {
  showPasswordGenModal.value = true;
  generateNewPassword();
}

function applyPwdPreset(preset: "strong" | "alphanumeric" | "pin" | "max") {
  if (preset === "strong") {
    pwdGenForm.value = { length: 20, uppercase: true, lowercase: true, numbers: true, symbols: true, exclude_ambiguous: false };
  } else if (preset === "alphanumeric") {
    pwdGenForm.value = { length: 18, uppercase: true, lowercase: true, numbers: true, symbols: false, exclude_ambiguous: false };
  } else if (preset === "pin") {
    pwdGenForm.value = { length: 6, uppercase: false, lowercase: false, numbers: true, symbols: false, exclude_ambiguous: false };
  } else if (preset === "max") {
    pwdGenForm.value = { length: 32, uppercase: true, lowercase: true, numbers: true, symbols: true, exclude_ambiguous: true };
  }
  generateNewPassword();
}

function copyGeneratedPassword() {
  if (!generatedPassword.value) return;
  navigator.clipboard.writeText(generatedPassword.value);
  notify("已成功复制新密码到剪贴板！", "success");
}

function applyPasswordToCurrentRecord() {
  if (!selectedRecord.value || !generatedPassword.value) return;
  selectedRecord.value.password = generatedPassword.value;
  invoke("save_record", { record: selectedRecord.value });
  showPasswordGenModal.value = false;
  notify(`已将生成的新密码更新至账号 [${selectedRecord.value.email}]`, "success", 3000);
  triggerDebouncedSync();
}

const passwordStrength = computed(() => {
  const p = generatedPassword.value;
  if (!p) return { score: 0, text: "无", color: "bg-gray-300", textCol: "text-gray-500", percent: "0%" };
  let score = 0;
  if (p.length >= 8) score += 1;
  if (p.length >= 16) score += 1;
  if (p.length >= 24) score += 1;
  if (/[a-z]/.test(p) && /[A-Z]/.test(p)) score += 1;
  if (/\d/.test(p)) score += 1;
  if (/[!@#$%&*+\-=?_]/.test(p)) score += 1;

  if (score <= 2) return { score: 1, text: "较弱", color: "bg-red-500", textCol: "text-red-600", percent: "25%" };
  if (score <= 4) return { score: 2, text: "中等", color: "bg-amber-500", textCol: "text-amber-600", percent: "50%" };
  if (score <= 5) return { score: 3, text: "高强度", color: "bg-blue-500", textCol: "text-blue-600", percent: "75%" };
  return { score: 4, text: "极强 (军工级强随机)", color: "bg-emerald-500", textCol: "text-emerald-600", percent: "100%" };
});

// Hardware & Gesture Back Navigation (Android Back Button / Back Gesture & Desktop Esc)
let lastBackPressTime = 0;
let backButtonListener: { unregister: () => Promise<void> } | null = null;

function handleBackAction(): boolean {
  // 1. Confirm dialog
  if (confirmState.value.visible) {
    confirmState.value.resolve(false);
    return true;
  }

  // 2. Settings modal
  if (showSettingsModal.value) {
    showSettingsModal.value = false;
    return true;
  }

  // 3. Password generator modal
  if (showPasswordGenModal.value) {
    showPasswordGenModal.value = false;
    return true;
  }

  // 4. Batch import modal
  if (showBatchImportModal.value) {
    showBatchImportModal.value = false;
    return true;
  }

  // 5. Custom URL shortcut modal
  if (showCustomUrlModal.value) {
    showCustomUrlModal.value = false;
    return true;
  }

  // 6. Backup codes modal
  if (showBackupCodesModal.value) {
    if (isEditingBackupCodesRaw.value) {
      isEditingBackupCodesRaw.value = false;
    } else {
      showBackupCodesModal.value = false;
    }
    return true;
  }

  // 7. Account add/edit modal
  if (isEditing.value) {
    isEditing.value = false;
    return true;
  }

  // 8. Mobile dropdown menu
  if (showMobileMoreMenu.value) {
    showMobileMoreMenu.value = false;
    return true;
  }

  // 9. Mobile search expanded
  if (showMobileSearch.value || searchQuery.value) {
    showMobileSearch.value = false;
    searchQuery.value = "";
    return true;
  }

  // 10. Mobile detail view -> return to account list
  if (mobileView.value === "detail") {
    mobileView.value = "list";
    selectedId.value = null;
    return true;
  }

  // 11. Escape key / Back navigation deselects currently selected account
  if (selectedId.value !== null) {
    selectedId.value = null;
    return true;
  }

  return false;
}

function handleKeyDown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    handleBackAction();
  }
}

async function setupBackNavigation() {
  if (typeof window !== "undefined") {
    window.addEventListener("keydown", handleKeyDown);
  }

  try {
    const { onBackButtonPress, exit } = await import("@tauri-apps/api/app");
    backButtonListener = await onBackButtonPress(async () => {
      const handled = handleBackAction();
      if (!handled) {
        const now = Date.now();
        if (now - lastBackPressTime < 2000) {
          try {
            await exit(0);
          } catch (e) {
            console.warn("Failed to exit app:", e);
          }
        } else {
          lastBackPressTime = now;
          notify("再按一次退出应用", "info", 2000);
        }
      }
    });
  } catch (err) {
    console.warn("Android back button registration not supported or failed on this platform:", err);
  }
}

function handleWindowFocus() {
  if (isUnlocked.value && webdavForm.value.auto_sync) {
    triggerSmartSync(true);
  }
}

let syncIntervalTimer: any = null;

// Lifecycle Hooks
onMounted(async () => {
  try {
    vaultExists.value = await invoke<boolean>("check_vault_exists");
  } catch (e) {
    vaultExists.value = false;
  }
  await refreshSecurityStatus();
  await setupBackNavigation();
  await loadWebDavConfig();
  if (typeof window !== "undefined") {
    window.addEventListener("resize", updateWindowWidth);
    window.addEventListener("focus", handleWindowFocus);
    updateWindowWidth();
  }

  syncIntervalTimer = setInterval(() => {
    if (isUnlocked.value && webdavForm.value.auto_sync) {
      triggerSmartSync(true);
    }
  }, 10 * 60 * 1000);

  try {
    unlistenUpdateProgress = await listen<{ downloaded: number; total: number; percent: number }>(
      "update-download-progress",
      (event) => {
        downloadProgress.value = event.payload.percent;
        if (event.payload.percent >= 100) {
          downloadStatusText.value = "下载完成！正在启动升级安装向导...";
        } else {
          const mb = (event.payload.downloaded / (1024 * 1024)).toFixed(1);
          const totalMb = (event.payload.total / (1024 * 1024)).toFixed(1);
          downloadStatusText.value = `正在下载更新包：${mb} MB / ${totalMb} MB (${event.payload.percent}%)`;
        }
      }
    );
  } catch (e) {
    console.warn("Failed to listen to update progress:", e);
  }
});

onUnmounted(() => {
  if (totpTimer) clearInterval(totpTimer);
  if (syncIntervalTimer) clearInterval(syncIntervalTimer);
  if (debouncedSyncTimer) clearTimeout(debouncedSyncTimer);
  if (typeof window !== "undefined") {
    window.removeEventListener("keydown", handleKeyDown);
    window.removeEventListener("resize", updateWindowWidth);
    window.removeEventListener("focus", handleWindowFocus);
  }
  if (backButtonListener) {
    backButtonListener.unregister().catch(() => {});
  }
  if (unlistenUpdateProgress) {
    unlistenUpdateProgress();
  }
});
</script>

<template>
  <div class="h-screen w-screen flex flex-col bg-[#f8fafd] select-none text-[#1f1f1f] font-sans antialiased overflow-hidden pt-[env(safe-area-inset-top,0px)] pb-[env(safe-area-inset-bottom,0px)]">
    
    <!-- 1. LOCK / UNLOCK SCREEN -->
    <div v-if="!isUnlocked" class="flex-1 flex items-center justify-center p-6 relative">
      <div class="w-full max-w-md bg-white rounded-3xl p-8 shadow-sm border border-[#dadce0]/70 flex flex-col items-center">
        <!-- Google Brand Logo -->
        <div class="w-16 h-16 rounded-2xl bg-white shadow-sm border border-[#dadce0]/70 flex items-center justify-center mb-5 p-2.5 overflow-hidden">
          <img src="/app.ico" alt="Google 账号管家 Logo" class="w-full h-full object-contain" />
        </div>
        <h1 class="text-2xl font-bold tracking-tight text-[#202124] mb-6">Google 账号管家</h1>

        <form @submit.prevent="handleAuth" class="w-full flex flex-col gap-4">
          <div>
            <label class="block text-xs font-semibold text-[#5f6368] uppercase tracking-wider mb-2">
              {{ vaultExists ? "输入主密码解锁" : "设置主密码初始化安全库" }}
            </label>
            <div class="relative">
              <input
                type="password"
                v-model="masterPassword"
                autofocus
                placeholder="请输入主解锁密码..."
                class="w-full px-4 py-3 bg-[#f1f3f4] hover:bg-[#e8eaed] focus:bg-white rounded-xl border-2 border-transparent focus:border-[#1a73e8] outline-none text-[#202124] transition-all text-sm"
              />
            </div>
          </div>

          <div v-if="unlockError" class="p-3 bg-red-50 text-red-600 rounded-xl text-xs flex items-center gap-2 border border-red-200">
            <AlertCircle class="w-4 h-4 flex-shrink-0" />
            <span>{{ unlockError }}</span>
          </div>

          <button
            type="submit"
            :disabled="isSubmitting"
            class="w-full mt-2 py-3.5 bg-[#1a73e8] hover:bg-[#1557b0] active:bg-[#174ea6] text-white font-medium rounded-xl shadow-sm hover:shadow transition-all flex items-center justify-center gap-2 text-sm disabled:opacity-50"
          >
            <Unlock class="w-4 h-4" />
            <span>{{ isSubmitting ? "正在验证..." : (vaultExists ? "解锁保险库" : "创建并开启保险库") }}</span>
          </button>
        </form>

        <!-- Fast Biometric / Windows Hello Fast Unlock Button -->
        <div v-if="helloStatus.enrolled && vaultExists" class="w-full flex flex-col items-center mt-3">
          <div class="w-full flex items-center my-2 text-xs text-[#80868b]">
            <div class="flex-1 border-t border-[#dadce0]"></div>
            <span class="px-3">或</span>
            <div class="flex-1 border-t border-[#dadce0]"></div>
          </div>

          <button
            type="button"
            @click="handleHelloUnlock"
            :disabled="isHelloVerifying || isSubmitting"
            class="w-full py-3 bg-[#e8f0fe] hover:bg-[#d2e3fc] active:bg-[#c2dcff] text-[#1a73e8] font-medium rounded-xl transition-all flex items-center justify-center gap-2.5 text-sm border border-[#1a73e8]/30 shadow-sm"
          >
            <Fingerprint class="w-5 h-5 text-[#1a73e8]" />
            <span>{{ isHelloVerifying ? (isMobilePlatform ? "请在手机弹窗中完成验证..." : "请在 Windows 弹窗中完成验证...") : (isMobilePlatform ? "快速解锁" : "使用 Windows Hello / PIN 快速解锁") }}</span>
          </button>
        </div>

        <div v-else-if="helloStatus.available && vaultExists" class="w-full mt-4 px-3.5 py-2.5 bg-[#f8fafd] border border-[#dadce0] rounded-xl text-xs text-[#5f6368] flex items-center gap-2">
          <Fingerprint class="w-4 h-4 text-[#1a73e8] flex-shrink-0" />
          <span>{{ isMobilePlatform ? "手机支持生物识别与锁屏密码，首次主密码解锁后可在设置中一键绑定快速登录。" : "本机支持 Windows Hello，首次主密码解锁后可在顶部一键绑定指纹/PIN快速登录。" }}</span>
        </div>


      </div>
    </div>

    <!-- 2. MAIN APPLICATION WORKSPACE -->
    <template v-else>
      <!-- 2.1 DESKTOP HEADER (md and above) -->
      <header class="hidden md:flex h-14 bg-white border-b border-[#dadce0] px-4 items-center justify-between gap-4 flex-shrink-0 z-10">
        <!-- Brand -->
        <div class="flex items-center gap-3">
          <div class="w-8 h-8 rounded-lg bg-white border border-[#dadce0]/60 flex items-center justify-center p-0.5 overflow-hidden shadow-xs">
            <img src="/app.ico" alt="Logo" class="w-full h-full object-contain" />
          </div>
          <div class="flex items-baseline gap-2">
            <span class="font-bold text-base text-[#202124] tracking-tight">Google 账号管家</span>
            <span class="text-[11px] font-semibold uppercase px-2 py-0.5 bg-[#e8f0fe] text-[#1a73e8] rounded-full">v3.1.15</span>
          </div>
        </div>

        <!-- Search Bar -->
        <div class="flex-1 max-w-md relative">
          <Search class="w-4 h-4 text-[#5f6368] absolute left-3.5 top-1/2 -translate-y-1/2" />
          <input
            type="text"
            v-model="searchQuery"
            placeholder="搜索邮箱、分组或备注 (Ctrl+F)..."
            class="w-full pl-9 pr-8 py-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] focus:bg-white rounded-full border border-transparent focus:border-[#1a73e8] outline-none text-xs text-[#202124] transition-all"
          />
          <button
            v-if="searchQuery"
            @click="searchQuery = ''"
            class="absolute right-3 top-1/2 -translate-y-1/2 text-[#5f6368] hover:text-[#202124] cursor-pointer"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>

        <!-- Right Toolbar Actions -->
        <div class="flex items-center gap-2">
          <button
            @click="openAddModal"
            class="h-9 px-3.5 bg-[#1a73e8] hover:bg-[#1557b0] active:bg-[#174ea6] text-white rounded-full text-xs font-medium flex items-center gap-1.5 shadow-sm transition-all cursor-pointer"
          >
            <Plus class="w-4 h-4" />
            <span>添加账号</span>
          </button>

          <button
            @click="openPasswordGenModal"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer"
            title="强随机密码生成器"
          >
            <KeyRound class="w-3.5 h-3.5 text-[#1a73e8]" />
            <span>密码生成器</span>
          </button>

          <button
            @click="openBatchImportModal"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer"
            title="批量导入多条账号"
          >
            <Upload class="w-3.5 h-3.5 text-[#1a73e8]" />
            <span>批量导入</span>
          </button>

          <!-- Cloud Smart Sync Indicator & Button -->
          <button
            @click="triggerSmartSync(false)"
            :disabled="syncState === 'syncing'"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all relative cursor-pointer disabled:opacity-75"
            :title="`WebDAV 增量智能同步: ${syncTooltip} (点击立即执行同步)`"
          >
            <RefreshCw
              v-if="syncState === 'syncing'"
              class="w-3.5 h-3.5 text-[#1a73e8] animate-spin"
            />
            <Cloud
              v-else
              :class="[
                'w-3.5 h-3.5',
                syncState === 'error' ? 'text-rose-500' :
                syncState === 'success' ? 'text-emerald-500' :
                webdavForm.url ? 'text-[#1a73e8]' : 'text-[#80868b]'
              ]"
            />
            <span>{{ syncState === 'syncing' ? '同步中' : syncState === 'error' ? '同步失败' : '云同步' }}</span>
            <span
              v-if="webdavForm.url && webdavForm.auto_sync"
              :class="[
                'w-1.5 h-1.5 rounded-full absolute top-2 right-2',
                syncState === 'syncing' ? 'bg-[#1a73e8] animate-ping' :
                syncState === 'error' ? 'bg-rose-500' :
                'bg-emerald-500'
              ]"
              :title="syncTooltip"
            ></span>
          </button>

          <!-- Unified Settings Button -->
          <button
            @click="openSettingsModal('security')"
            class="h-9 px-3.5 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all relative cursor-pointer"
            title="设置与偏好 (Windows Hello、云端同步、备份导出、检查更新)"
          >
            <Settings class="w-3.5 h-3.5 text-[#1a73e8]" />
            <span>设置</span>
            <span v-if="helloStatus.enrolled" class="w-1.5 h-1.5 rounded-full bg-emerald-500 absolute top-2 right-2" title="Windows Hello 已开启"></span>
          </button>

          <button
            @click="lockVault"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all cursor-pointer"
            title="锁定保险库"
          >
            <Lock class="w-3.5 h-3.5" />
            <span>锁定</span>
          </button>
        </div>
      </header>

      <!-- 2.2 MOBILE HEADER (< md) -->
      <header class="md:hidden bg-white border-b border-[#dadce0] flex flex-col flex-shrink-0 z-20">
        <!-- Mobile Main Nav Row -->
        <div class="h-13 px-3.5 flex items-center justify-between gap-2">
          <!-- Left: Brand -->
          <div class="flex items-center gap-2 min-w-0">
            <div class="w-7 h-7 rounded-lg bg-white border border-[#dadce0]/60 flex items-center justify-center p-0.5 overflow-hidden shadow-2xs flex-shrink-0">
              <img src="/app.ico" alt="Logo" class="w-full h-full object-contain" />
            </div>
            <span class="font-bold text-sm text-[#202124] tracking-tight whitespace-nowrap">Google 账号管家</span>
          </div>

          <!-- Right: Mobile Action Buttons -->
          <div class="flex items-center gap-1 flex-shrink-0">
            <!-- Search Toggle Button -->
            <button
              @click="showMobileSearch = !showMobileSearch"
              class="w-8 h-8 rounded-full flex items-center justify-center text-[#5f6368] hover:bg-[#f1f3f4] active:bg-[#e8eaed] transition-colors cursor-pointer"
              :class="{ 'bg-[#e8f0fe] text-[#1a73e8]': showMobileSearch || searchQuery }"
              title="搜索"
            >
              <Search class="w-4 h-4" />
            </button>

            <!-- Quick Add Button -->
            <button
              @click="openAddModal"
              class="h-8 px-2.5 bg-[#1a73e8] text-white rounded-full text-xs font-semibold flex items-center gap-1 shadow-xs active:bg-[#1557b0] transition-colors cursor-pointer"
              title="添加新账号"
            >
              <Plus class="w-3.5 h-3.5" />
              <span>添加</span>
            </button>

            <!-- Mobile Cloud Sync Button -->
            <button
              @click="triggerSmartSync(false)"
              :disabled="syncState === 'syncing'"
              class="w-8 h-8 rounded-full flex items-center justify-center text-[#5f6368] hover:bg-[#f1f3f4] active:bg-[#e8eaed] transition-colors relative cursor-pointer disabled:opacity-75"
              :title="`WebDAV 云端同步: ${syncTooltip} (点击同步)`"
            >
              <RefreshCw
                v-if="syncState === 'syncing'"
                class="w-4 h-4 text-[#1a73e8] animate-spin"
              />
              <Cloud
                v-else
                :class="[
                  'w-4 h-4',
                  syncState === 'error' ? 'text-rose-500' :
                  syncState === 'success' ? 'text-emerald-500' :
                  webdavForm.url ? 'text-[#1a73e8]' : 'text-[#80868b]'
                ]"
              />
              <span
                v-if="webdavForm.url && webdavForm.auto_sync"
                :class="[
                  'w-1.5 h-1.5 rounded-full absolute top-1.5 right-1.5',
                  syncState === 'syncing' ? 'bg-[#1a73e8] animate-ping' :
                  syncState === 'error' ? 'bg-rose-500' :
                  'bg-emerald-500'
                ]"
              ></span>
            </button>

            <!-- Settings Button -->
            <button
              @click="openSettingsModal('security')"
              class="w-8 h-8 rounded-full flex items-center justify-center text-[#5f6368] hover:bg-[#f1f3f4] active:bg-[#e8eaed] transition-colors relative cursor-pointer"
              title="设置"
            >
              <Settings class="w-4 h-4 text-[#1a73e8]" />
              <span v-if="helloStatus.enrolled" class="w-1.5 h-1.5 rounded-full bg-emerald-500 absolute top-1.5 right-1.5"></span>
            </button>

            <!-- More Actions Dropdown Toggle -->
            <div class="relative">
              <button
                @click="showMobileMoreMenu = !showMobileMoreMenu"
                class="w-8 h-8 rounded-full flex items-center justify-center text-[#5f6368] hover:bg-[#f1f3f4] active:bg-[#e8eaed] transition-colors cursor-pointer"
                title="更多菜单"
              >
                <MoreVertical class="w-4 h-4" />
              </button>

              <!-- More Actions Dropdown Menu -->
              <div
                v-if="showMobileMoreMenu"
                class="absolute right-0 top-10 w-44 bg-white rounded-2xl shadow-xl border border-[#dadce0] p-1.5 z-50 text-xs font-medium space-y-0.5 animate-fade-in"
              >
                <button
                  @click="showMobileMoreMenu = false; triggerSmartSync(false)"
                  class="w-full px-3 py-2 text-left hover:bg-[#f1f3f4] rounded-xl flex items-center gap-2 text-[#3c4043] cursor-pointer"
                >
                  <Cloud class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>立即智能同步</span>
                </button>
                <button
                  @click="showMobileMoreMenu = false; openPasswordGenModal()"
                  class="w-full px-3 py-2 text-left hover:bg-[#f1f3f4] rounded-xl flex items-center gap-2 text-[#3c4043] cursor-pointer"
                >
                  <KeyRound class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>密码生成器</span>
                </button>
                <button
                  @click="showMobileMoreMenu = false; openBatchImportModal()"
                  class="w-full px-3 py-2 text-left hover:bg-[#f1f3f4] rounded-xl flex items-center gap-2 text-[#3c4043] cursor-pointer"
                >
                  <Upload class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>批量导入</span>
                </button>
                <div class="border-t border-[#f1f3f4] my-1"></div>
                <button
                  @click="showMobileMoreMenu = false; lockVault()"
                  class="w-full px-3 py-2 text-left hover:bg-rose-50 text-rose-600 rounded-xl flex items-center gap-2 cursor-pointer"
                >
                  <Lock class="w-3.5 h-3.5" />
                  <span>锁定保险库</span>
                </button>
              </div>
            </div>
          </div>
        </div>

        <!-- Mobile Search Input Row (collapsible) -->
        <div v-if="showMobileSearch || searchQuery" class="px-3 pb-2 pt-0 flex items-center gap-2">
          <div class="flex-1 relative">
            <Search class="w-3.5 h-3.5 text-[#5f6368] absolute left-3 top-1/2 -translate-y-1/2" />
            <input
              type="text"
              v-model="searchQuery"
              placeholder="搜索邮箱、分组或备注..."
              class="w-full pl-8 pr-7 py-1.5 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] outline-none text-xs text-[#202124]"
            />
            <button
              v-if="searchQuery"
              @click="searchQuery = ''"
              class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[#5f6368] cursor-pointer"
            >
              <X class="w-3 h-3" />
            </button>
          </div>
        </div>

        <!-- Mobile Horizontal Group Selector Pills -->
        <div class="flex items-center gap-1.5 px-3 py-1.5 bg-[#f8fafd] border-t border-[#dadce0]/60 overflow-x-auto no-scrollbar flex-shrink-0">
          <button
            v-for="g in groups"
            :key="g.label"
            @click="currentGroup = g.key; mobileView = 'list'"
            :class="[
              'px-2.5 py-1 rounded-full text-xs whitespace-nowrap flex items-center gap-1 transition-all flex-shrink-0 font-medium cursor-pointer',
              currentGroup === g.key
                ? 'bg-[#1a73e8] text-white font-semibold shadow-2xs'
                : 'bg-white text-[#5f6368] border border-[#dadce0] hover:bg-[#f1f3f4]'
            ]"
          >
            <span>{{ g.label }}</span>
            <span class="text-[10px] opacity-80 font-mono">({{ g.count }})</span>
          </button>
        </div>
      </header>

      <!-- THREE-COLUMN MAIN BODY -->
      <div class="flex-1 flex overflow-hidden relative">
        
        <!-- COLUMN 1: SIDEBAR (Desktop only) -->
        <aside class="hidden md:flex w-56 bg-white border-r border-[#dadce0] flex-col flex-shrink-0">
          <div class="px-4 py-3 border-b border-[#f1f3f4]">
            <h2 class="text-[11px] font-bold text-[#5f6368] uppercase tracking-wider">账号分组</h2>
          </div>

          <div class="flex-1 overflow-y-auto p-2 space-y-0.5">
            <button
              v-for="g in groups"
              :key="g.label"
              @click="currentGroup = g.key"
              :class="[
                'w-full px-3 py-2 rounded-xl text-xs font-medium flex items-center justify-between transition-colors',
                currentGroup === g.key
                  ? 'bg-[#e8f0fe] text-[#1a73e8] font-semibold'
                  : 'text-[#444746] hover:bg-[#f1f3f4]'
              ]"
            >
              <div class="flex items-center gap-2 truncate">
                <Folder class="w-3.5 h-3.5 flex-shrink-0 text-[#5f6368]" />
                <span class="truncate">{{ g.label }}</span>
              </div>
              <span class="text-[11px] text-[#5f6368] bg-black/5 px-2 py-0.5 rounded-full font-mono">{{ g.count }}</span>
            </button>
          </div>

          <!-- Bottom Summary Box -->
          <div class="p-3 border-t border-[#dadce0] bg-[#f8fafd]">
            <div class="text-[11px] text-[#5f6368] space-y-1">
              <div>已保存总数：<strong class="text-[#202124]">{{ records.length }}</strong> 个账号</div>
              <div class="text-[10px] text-[#80868b]">AES-256-GCM 本地加密已就绪</div>
            </div>
          </div>
        </aside>

        <!-- COLUMN 2: MIDDLE ACCOUNT LIST -->
        <main
          class="bg-white border-r border-[#dadce0] flex flex-col overflow-hidden"
          :class="[
            mobileView === 'detail' ? 'hidden md:flex' : 'flex flex-1 w-full',
            'md:flex md:flex-1'
          ]"
        >
          <div class="px-4 py-2 bg-[#f8fafd] border-b border-[#dadce0] flex items-center justify-between text-xs text-[#5f6368]">
            <span>共 {{ filteredRecords.length }} 个账号符合筛选</span>
            <span class="text-[11px] text-[#80868b] hidden md:inline">双击可快速编辑详情</span>
          </div>

          <div class="flex-1 overflow-y-auto divide-y divide-[#f1f3f4]" @click.self="selectedId = null">
            <div
              v-for="r in filteredRecords"
              :key="r.id"
              @click="selectAccount(r.id)"
              @dblclick="openEditModal(r)"
              :class="[
                'px-4 py-3 flex items-center justify-between cursor-pointer transition-colors',
                selectedId === r.id ? 'bg-[#edf2fa]' : 'hover:bg-[#f8fafd]'
              ]"
            >
              <!-- Left: Avatar + Email + Group -->
              <div class="flex items-center gap-3 min-w-0">
                <div class="w-9 h-9 rounded-full bg-[#e8f0fe] text-[#1a73e8] font-bold text-sm flex items-center justify-center flex-shrink-0">
                  {{ (r.email[0] || 'G').toUpperCase() }}
                </div>
                <div class="min-w-0">
                  <div class="text-xs font-semibold text-[#202124] truncate flex items-center gap-1.5">
                    <span class="truncate">{{ r.email }}</span>
                    <span :class="getStatusBadgeClass(r.status)">{{ r.status || '待录入' }}</span>
                  </div>
                  <div class="text-[11px] text-[#5f6368] flex items-center gap-2 mt-0.5">
                    <span v-if="r.group" class="px-1.5 py-0.2 bg-[#f1f3f4] text-[#444746] rounded text-[10px]">{{ r.group }}</span>
                    <span v-if="r.google_one_plan" class="px-1.5 py-0.2 bg-[#e8f0fe] text-[#1a73e8] rounded text-[10px] font-mono">{{ r.google_one_plan }}</span>
                    <span v-if="r.recovery_email" class="truncate">{{ r.recovery_email }}</span>
                  </div>
                </div>
              </div>

              <!-- Right: TOTP & One Badge -->
              <div class="flex items-center gap-2 md:gap-4 flex-shrink-0">
                <!-- 2FA Code Badge -->
                <div v-if="r.totp" class="flex flex-col items-end">
                  <div class="flex items-center gap-1.5 font-mono text-sm font-bold text-[#1a73e8]">
                    <span>{{ totpMap[r.id]?.code || "••••••" }}</span>
                    <button
                      @click.stop="copyText(totpMap[r.id]?.code || '', r.id + '-totp')"
                      class="text-[#5f6368] hover:text-[#1a73e8] cursor-pointer"
                      title="复制动态验证码"
                    >
                      <Check v-if="copiedField === r.id + '-totp'" class="w-3.5 h-3.5 text-emerald-600" />
                      <Copy v-else class="w-3.5 h-3.5" />
                    </button>
                  </div>
                  <span class="text-[10px] text-[#80868b] font-mono">{{ totpMap[r.id]?.remaining_seconds || 30 }}s 剩余</span>
                </div>
                <div v-else class="text-[11px] text-[#9aa0a6] font-mono hidden sm:block">未配 2FA</div>

                <!-- Google One Badge -->
                <div
                  v-if="r.google_one_status === 'active'"
                  class="px-2 py-0.5 rounded-full text-[10px] font-medium bg-emerald-50 text-emerald-700 border border-emerald-200"
                >
                  One 有效
                </div>
                <div
                  v-else-if="r.google_one_status === 'expired'"
                  class="px-2 py-0.5 rounded-full text-[10px] font-medium bg-rose-50 text-rose-700 border border-rose-200"
                >
                  已失效
                </div>
                <div
                  v-else-if="r.google_one_status === 'inactive'"
                  class="px-2 py-0.5 rounded-full text-[10px] font-medium bg-gray-100 text-gray-600 hidden sm:block"
                >
                  非会员
                </div>
                <div
                  v-else
                  class="px-2 py-0.5 rounded-full text-[10px] font-medium bg-[#f1f3f4] text-[#5f6368] hidden sm:block"
                >
                  未验证
                </div>
              </div>
            </div>

            <div v-if="filteredRecords.length === 0" class="p-8 text-center text-xs text-[#80868b]">
              暂无匹配的账号记录
            </div>
          </div>

          <!-- Mobile Bottom Summary Bar -->
          <div class="md:hidden px-4 py-2.5 bg-[#f8fafd] border-t border-[#dadce0] text-[11px] text-[#5f6368] flex items-center justify-between flex-shrink-0">
            <span>已保存 <strong>{{ records.length }}</strong> 个账号</span>
            <span class="text-[10px] text-[#80868b]">AES-256-GCM 本地加密就绪</span>
          </div>
        </main>

        <!-- Resizer Divider (Desktop only) -->
        <div
          class="hidden md:flex items-center justify-center w-1 hover:w-1.5 bg-[#dadce0]/40 hover:bg-[#1a73e8]/40 active:bg-[#1a73e8] cursor-col-resize transition-all select-none z-10 -ml-0.5"
          :class="{ 'bg-[#1a73e8] w-1.5': isResizingDetail }"
          @mousedown="startResizeDetail"
          @dblclick="resetDetailWidth"
          title="按住鼠标左右拖拽调整详情栏宽度，双击恢复默认宽度"
        >
          <div class="w-0.5 h-6 bg-[#dadce0] rounded-full"></div>
        </div>

        <!-- COLUMN 3: RIGHT DETAIL CARD -->
        <aside
          class="bg-[#f8fafd] flex flex-col overflow-y-auto p-3 gap-2.5 border-l border-[#dadce0]/70 flex-shrink-0"
          :class="[
            mobileView === 'detail' ? 'flex flex-1 w-full' : 'hidden md:flex',
            'md:flex md:flex-none'
          ]"
          :style="isDesktopWindow && !isMobilePlatform ? { width: `${detailWidth}px` } : {}"
        >
          <template v-if="selectedRecord">
            <!-- Mobile Back Navigation Header -->
            <div class="md:hidden flex items-center justify-between pb-2 border-b border-[#dadce0]/70 flex-shrink-0">
              <button
                @click="mobileView = 'list'"
                class="px-3 py-1.5 bg-white active:bg-[#f1f3f4] text-[#1a73e8] border border-[#dadce0] rounded-xl text-xs font-semibold flex items-center gap-1 transition-colors shadow-2xs cursor-pointer"
              >
                <ChevronLeft class="w-4 h-4" />
                <span>返回账号列表</span>
              </button>

              <div class="flex items-center gap-1.5">
                <button
                  @click="openEditModal(selectedRecord)"
                  class="px-2.5 py-1.5 bg-white text-[#5f6368] hover:text-[#1a73e8] border border-[#dadce0] rounded-xl text-xs font-medium flex items-center gap-1 cursor-pointer"
                >
                  <Edit3 class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>编辑</span>
                </button>
                <button
                  @click="handleDeleteRecord(selectedRecord.id)"
                  class="px-2.5 py-1.5 bg-white text-rose-600 border border-[#dadce0] rounded-xl text-xs font-medium flex items-center gap-1 cursor-pointer"
                >
                  <Trash2 class="w-3.5 h-3.5" />
                  <span>删除</span>
                </button>
              </div>
            </div>

            <!-- Account Header Card -->
            <div class="bg-white rounded-2xl p-3 border border-[#dadce0]/80 shadow-2xs">
              <div class="flex items-center gap-2.5 mb-2.5">
                <div class="w-9 h-9 rounded-full bg-[#e8f0fe] text-[#1a73e8] font-bold text-base flex items-center justify-center flex-shrink-0">
                  {{ (selectedRecord.email[0] || 'G').toUpperCase() }}
                </div>
                <div class="min-w-0 flex-1">
                  <div class="flex items-center justify-between gap-1.5">
                    <div class="text-[11px] text-[#5f6368]">Google 账号</div>
                    <div class="flex items-center gap-1.5">
                      <span :class="getStatusBadgeClass(selectedRecord.status)">{{ selectedRecord.status || '待录入' }}</span>
                      <button
                        @click="selectedId = null"
                        class="hidden md:flex p-0.5 hover:bg-[#f1f3f4] text-[#80868b] hover:text-[#202124] rounded-md transition-colors cursor-pointer"
                        title="取消选中 / 返回防窥模式 (Esc)"
                      >
                        <X class="w-3.5 h-3.5" />
                      </button>
                    </div>
                  </div>
                  <div class="text-xs font-bold text-[#202124] truncate" :title="selectedRecord.email">{{ selectedRecord.email }}</div>
                </div>
              </div>

              <!-- Quick Launch Buttons (Shape A: In-App Isolated Windows) -->
              <div class="grid grid-cols-3 gap-1.5">
                <button
                  @click="openInApp(`https://accounts.google.com/ServiceLogin?Email=${encodeURIComponent(selectedRecord.email)}&login_hint=${encodeURIComponent(selectedRecord.email)}&continue=${encodeURIComponent('https://myaccount.google.com/')}`)"
                  :disabled="isOpeningBrowser"
                  class="h-7 px-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-[11px] font-medium flex items-center justify-center gap-1 transition-all disabled:opacity-50 cursor-pointer"
                  title="在专属独立会话窗口中登录此账号"
                >
                  <ExternalLink class="w-3 h-3 text-[#1a73e8]" />
                  <span>快捷登录</span>
                </button>
                <button
                  @click="openInApp('https://one.google.com/storage')"
                  :disabled="isOpeningBrowser"
                  class="h-7 px-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-[11px] font-medium flex items-center justify-center gap-1 transition-all disabled:opacity-50 cursor-pointer"
                  title="在专属独立会话窗口中打开 Google One"
                >
                  <Globe class="w-3 h-3 text-[#1a73e8]" />
                  <span>One 方案</span>
                </button>
                <button
                  @click="openInApp('https://policies.google.com/terms')"
                  :disabled="isOpeningBrowser"
                  class="h-7 px-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-[11px] font-medium flex items-center justify-center gap-1 transition-all disabled:opacity-50 cursor-pointer"
                  title="在专属独立会话窗口中查看注册归属地区"
                >
                  <Info class="w-3 h-3 text-[#1a73e8]" />
                  <span>归属地区</span>
                </button>
                <button
                  @click="openInApp('https://payments.google.com/gp/w/home/settings')"
                  :disabled="isOpeningBrowser"
                  class="h-7 px-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-[11px] font-medium flex items-center justify-center gap-1 transition-all col-span-2 disabled:opacity-50 cursor-pointer"
                  title="在专属独立会话窗口中打开支付与账单"
                >
                  <CreditCard class="w-3 h-3 text-[#1a73e8]" />
                  <span>支付设置与账单</span>
                </button>
                <button
                  @click="openCustomUrlModal"
                  class="h-7 px-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-[11px] font-medium flex items-center justify-center gap-1 transition-all cursor-pointer"
                  title="在独立窗口中打开其他服务或自定义网址"
                >
                  <Compass class="w-3 h-3 text-[#1a73e8]" />
                  <span>更多网址</span>
                </button>
                <button
                  v-if="selectedRecord.recovery_email"
                  @click="copyText(selectedRecord.recovery_email, 'rec_email')"
                  class="h-7 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-[11px] font-medium flex items-center justify-center gap-1 transition-all col-span-3 truncate cursor-pointer"
                >
                  <Copy class="w-3 h-3 flex-shrink-0" />
                  <span class="truncate">{{ copiedField === 'rec_email' ? '已复制辅助邮箱' : '复制辅助邮箱：' + selectedRecord.recovery_email }}</span>
                </button>
              </div>
            </div>

            <!-- Credentials & Structured Info -->
            <div class="bg-white rounded-2xl p-3 border border-[#dadce0]/80 shadow-2xs space-y-2.5">
              <!-- Password Row -->
              <div class="flex items-start justify-between gap-2.5 pb-2 border-b border-[#f1f3f4]">
                <div class="min-w-0 flex-1">
                  <div class="text-[10px] text-[#5f6368]">登录密码</div>
                  <div class="font-mono text-xs font-semibold text-[#202124] tracking-wide mt-0.5 select-all break-all leading-relaxed">
                    {{ showPassword ? (selectedRecord.password || "未记录") : "••••••••••••" }}
                  </div>
                </div>
                <div class="flex items-center gap-1 flex-shrink-0 pt-0.5">
                  <button
                    @click="showPassword = !showPassword"
                    class="p-1.5 hover:bg-[#f1f3f4] rounded-lg text-[#5f6368] transition-colors cursor-pointer"
                    title="显示 / 隐藏"
                  >
                    <EyeOff v-if="showPassword" class="w-3.5 h-3.5" />
                    <Eye v-else class="w-3.5 h-3.5" />
                  </button>
                  <button
                    @click="copyText(selectedRecord.password || '', 'pwd')"
                    class="p-1.5 hover:bg-[#f1f3f4] rounded-lg text-[#5f6368] transition-colors cursor-pointer"
                    title="复制密码"
                  >
                    <Check v-if="copiedField === 'pwd'" class="w-3.5 h-3.5 text-emerald-600" />
                    <Copy v-else class="w-3.5 h-3.5" />
                  </button>
                </div>
              </div>

              <!-- Information Grid -->
              <div class="grid grid-cols-2 gap-y-1.5 text-xs">
                <div class="text-[#5f6368]">所属分组</div>
                <div class="text-[#202124] font-medium truncate">{{ selectedRecord.group || "未分组" }}</div>

                <div class="text-[#5f6368]">账号状态</div>
                <div>
                  <span :class="getStatusBadgeClass(selectedRecord.status)">{{ selectedRecord.status || '待录入' }}</span>
                </div>

                <div class="text-[#5f6368]">恢复邮箱</div>
                <div class="text-[#1a73e8] font-medium truncate" :title="selectedRecord.recovery_email">{{ selectedRecord.recovery_email || "—" }}</div>

                <div class="text-[#5f6368]">恢复手机</div>
                <div class="text-[#202124] font-medium truncate">{{ selectedRecord.phone || "—" }}</div>

                <div class="text-[#5f6368]">验证方式</div>
                <div class="text-[#202124] font-medium truncate">{{ selectedRecord.verification || "—" }}</div>

                <div class="text-[#5f6368]">Google One</div>
                <div class="text-[#202124] font-medium truncate">
                  {{ selectedRecord.google_one_plan ? (selectedRecord.google_one_plan + (selectedRecord.google_one_used ? ' (' + selectedRecord.google_one_used + ')' : '')) : '—' }}
                </div>
              </div>

              <!-- Notes -->
              <div v-if="selectedRecord.notes" class="pt-2 border-t border-[#f1f3f4] text-xs text-[#5f6368]">
                <div class="font-medium text-[11px] mb-1">备注信息：</div>
                <div class="p-2 bg-[#f8fafd] rounded-xl text-[#202124] text-[11px] leading-relaxed whitespace-pre-wrap">{{ selectedRecord.notes }}</div>
              </div>
            </div>

            <!-- TOTP Card -->
            <div class="bg-[#edf2fa] rounded-2xl p-3 border border-[#d3e3fd]/80">
              <div class="flex items-center justify-between mb-1.5">
                <span class="text-xs font-bold text-[#444746] flex items-center gap-1.5">
                  <Clock class="w-3.5 h-3.5 text-[#1a73e8]" />
                  2FA 动态安全码 (TOTP)
                </span>
                <span class="text-[11px] font-mono text-[#5f6368]">{{ totpMap[selectedRecord.id]?.remaining_seconds || 30 }}s 后刷新</span>
              </div>

              <div class="flex items-center justify-between my-1">
                <div class="font-mono text-2xl font-black tracking-widest text-[#1a73e8]">
                  {{ totpMap[selectedRecord.id]?.code || "未配置" }}
                </div>
                <button
                  v-if="selectedRecord.totp"
                  @click="copyText(totpMap[selectedRecord.id]?.code || '', 'totp_large')"
                  class="px-2.5 py-1 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center gap-1 shadow-2xs transition-all cursor-pointer"
                >
                  <Check v-if="copiedField === 'totp_large'" class="w-3.5 h-3.5" />
                  <Copy v-else class="w-3.5 h-3.5" />
                  <span>{{ copiedField === 'totp_large' ? '已复制' : '复制' }}</span>
                </button>
              </div>
            </div>

            <!-- BACKUP CODES CARD -->
            <div class="bg-white rounded-2xl p-3 border border-[#dadce0]/80 shadow-2xs space-y-2">
              <div class="flex items-center justify-between">
                <span class="text-xs font-bold text-[#202124] flex items-center gap-1.5">
                  <KeyRound class="w-3.5 h-3.5 text-[#1a73e8]" />
                  Google 备用验证码
                </span>
                <span
                  v-if="parsedBackupCodesList.length > 0"
                  :class="[
                    'text-[10px] font-semibold px-2 py-0.5 rounded-full font-mono',
                    unusedBackupCodesCount > 0 ? 'bg-emerald-50 text-emerald-700 border border-emerald-200' : 'bg-rose-50 text-rose-700 border border-rose-200'
                  ]"
                >
                  剩余 {{ unusedBackupCodesCount }} / {{ parsedBackupCodesList.length }}
                </span>
              </div>

              <div v-if="parsedBackupCodesList.length > 0" class="space-y-1.5">
                <button
                  @click="copyAndConsumeNextBackupCode"
                  :disabled="unusedBackupCodesCount === 0"
                  class="w-full py-1.5 bg-[#1a73e8] hover:bg-[#1557b0] active:bg-[#174ea6] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-2xs transition-all disabled:opacity-50 cursor-pointer"
                  title="自动复制下一个未使用的备用码，并立即标记为已使用"
                >
                  <Check v-if="copiedField === 'backup_code_consumed'" class="w-3.5 h-3.5" />
                  <Copy v-else class="w-3.5 h-3.5" />
                  <span>{{ copiedField === 'backup_code_consumed' ? '已复制并自动核销！' : '⚡ 复制下一个可用码并核销' }}</span>
                </button>

                <button
                  @click="openBackupCodesModal"
                  class="w-full py-1 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium flex items-center justify-center gap-1 transition-all cursor-pointer"
                >
                  <span>查看全部备用码与核销状态</span>
                </button>
              </div>

              <div v-else class="text-center py-1.5">
                <p class="text-[11px] text-[#80868b] mb-1.5">尚未配置备用验证码</p>
                <button
                  @click="openBackupCodesModal"
                  class="px-2.5 py-1 bg-[#e8f0fe] hover:bg-[#d2e3fc] text-[#1a73e8] rounded-lg text-xs font-medium transition-all cursor-pointer"
                >
                  + 批量粘贴备用码
                </button>
              </div>
            </div>

            <!-- Action Buttons -->
            <div class="space-y-1.5 pt-1">
              <button
                @click="openEditModal(selectedRecord)"
                class="w-full py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-2xs transition-all cursor-pointer"
              >
                <Edit3 class="w-3.5 h-3.5" />
                <span>编辑该账号资料</span>
              </button>
              <button
                @click="handleDeleteRecord(selectedRecord.id)"
                class="w-full py-1.5 text-rose-600 hover:bg-rose-50 rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-colors cursor-pointer"
              >
                <Trash2 class="w-3 h-3" />
                <span>删除此本地记录</span>
              </button>
            </div>
          </template>

          <div v-else class="flex-1 flex flex-col items-center justify-center text-center p-6 text-[#80868b] select-none">
            <div class="w-14 h-14 rounded-2xl bg-[#e8f0fe] flex items-center justify-center mb-3 text-[#1a73e8] shadow-2xs">
              <Shield class="w-7 h-7" />
            </div>
            <h4 class="text-sm font-bold text-[#202124] mb-1">隐私防窥模式已就绪</h4>
            <p class="text-xs text-[#5f6368] max-w-xs leading-relaxed mb-4">
              当前未选中任何账号，密码与 2FA 动态码处于防窥保护中。请在左侧列表中点击选择查看。
            </p>
            <div class="inline-flex items-center gap-1.5 text-[11px] text-[#5f6368] bg-white px-3 py-1.5 rounded-xl border border-[#dadce0]/80 shadow-2xs">
              <span>快捷提示:</span>
              <kbd class="px-1.5 py-0.5 bg-[#f1f3f4] text-[#3c4043] rounded border border-[#dadce0] text-[10px] font-mono font-medium">再次点击</kbd>
              <span>或</span>
              <kbd class="px-1.5 py-0.5 bg-[#f1f3f4] text-[#3c4043] rounded border border-[#dadce0] text-[10px] font-mono font-medium">Esc</kbd>
              <span>可随时取消选中</span>
            </div>
          </div>
        </aside>

      </div>
    </template>

    <!-- 3. ADD / EDIT MODAL -->
    <div
      v-if="isEditing"
      class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="w-full max-w-lg bg-white rounded-3xl p-6 shadow-xl border border-[#dadce0] flex flex-col max-h-[90vh]">
        <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4] mb-4">
          <h2 class="text-base font-bold text-[#202124]">
            {{ editForm.id ? "编辑账号资料" : "添加新账号" }}
          </h2>
          <button @click="isEditing = false" class="p-1.5 hover:bg-[#f1f3f4] rounded-full text-[#5f6368]">
            <X class="w-4 h-4" />
          </button>
        </div>

        <form @submit.prevent="handleSaveRecord" class="flex-1 overflow-y-auto space-y-4 pr-1">
          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">Google 邮箱 *</label>
            <input
              type="email"
              required
              v-model="editForm.email"
              placeholder="example@gmail.com"
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
            />
          </div>

          <div>
            <div class="flex items-center justify-between mb-1">
              <label class="block text-xs font-semibold text-[#5f6368]">登录密码</label>
              <button
                type="button"
                @click="generatePassword"
                class="text-[11px] text-[#1a73e8] hover:underline flex items-center gap-1"
              >
                <Sparkles class="w-3 h-3" />
                随机强密码
              </button>
            </div>
            <input
              type="text"
              v-model="editForm.password"
              placeholder="请输入密码..."
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-mono"
            />
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">所属分组</label>
              <input
                type="text"
                v-model="editForm.group"
                placeholder="工作 / 个人 / 测试..."
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              />
            </div>
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">账号状态</label>
              <select
                v-model="editForm.status"
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-medium text-[#202124]"
              >
                <option v-for="st in accountStatusOptions" :key="st" :value="st">{{ st }}</option>
              </select>
            </div>
          </div>

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">恢复邮箱</label>
              <input
                type="text"
                v-model="editForm.recovery_email"
                placeholder="backup@outlook.com"
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              />
            </div>
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">恢复手机号</label>
              <input
                type="text"
                v-model="editForm.phone"
                placeholder="+86 13800..."
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              />
            </div>
          </div>

          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">2FA 密钥 (Base32 或 otpauth://)</label>
            <input
              type="text"
              v-model="editForm.totp"
              placeholder="JBSWY3DPEHPK3PXP..."
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-mono"
            />
          </div>

          <div class="grid grid-cols-1 sm:grid-cols-3 gap-3">
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">Google One 方案</label>
              <select
                v-model="editForm.google_one_plan"
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-medium text-[#202124]"
              >
                <option v-for="plan in googleOnePlanOptions" :key="plan" :value="plan">{{ plan }}</option>
                <option v-if="editForm.google_one_plan && !googleOnePlanOptions.includes(editForm.google_one_plan)" :value="editForm.google_one_plan">
                  {{ editForm.google_one_plan }} (自定义)
                </option>
              </select>
            </div>
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">Google One 状态</label>
              <select
                v-model="editForm.google_one_status"
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              >
                <option value="unknown">未验证</option>
                <option value="active">有效（手动标记）</option>
                <option value="expired">已失效</option>
                <option value="inactive">非会员</option>
              </select>
            </div>
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">已用容量</label>
              <input
                type="text"
                v-model="editForm.google_one_used"
                placeholder="例如 15 GB / 350 GB"
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              />
            </div>
          </div>

          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">备注</label>
            <textarea
              v-model="editForm.notes"
              rows="2"
              placeholder="添加任何重要记录..."
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none resize-none"
            ></textarea>
          </div>

          <div class="flex items-center justify-end gap-2 pt-3 border-t border-[#f1f3f4]">
            <button
              type="button"
              @click="isEditing = false"
              :disabled="isSavingRecord"
              class="px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#444746] rounded-xl text-xs font-medium disabled:opacity-50"
            >
              取消
            </button>
            <button
              type="submit"
              :disabled="isSavingRecord"
              class="px-5 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium shadow-sm disabled:opacity-50 flex items-center gap-1.5"
            >
              <RefreshCw v-if="isSavingRecord" class="w-3.5 h-3.5 animate-spin" />
              <span>{{ isSavingRecord ? "正在加密保存..." : "保存至安全库" }}</span>
            </button>
          </div>
        </form>
      </div>
    </div>

    <!-- 4. CUSTOM URL & PRESET SERVICES MODAL (Shape A) -->
    <div
      v-if="showCustomUrlModal && selectedRecord"
      class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="w-full max-w-md bg-white rounded-3xl p-6 shadow-xl border border-[#dadce0] flex flex-col">
        <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4] mb-4">
          <div>
            <h2 class="text-base font-bold text-[#202124]">独立会话浏览器</h2>
            <p class="text-xs text-[#5f6368] mt-0.5">当前账号：<span class="font-medium text-[#1a73e8]">{{ selectedRecord.email }}</span></p>
          </div>
          <button @click="showCustomUrlModal = false" class="p-1.5 hover:bg-[#f1f3f4] rounded-full text-[#5f6368]">
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Quick Presets -->
        <div class="mb-4">
          <label class="block text-xs font-semibold text-[#5f6368] mb-2 uppercase tracking-wider">常用服务直达</label>
          <div class="grid grid-cols-2 gap-2">
            <button
              v-for="p in servicePresets"
              :key="p.url"
              type="button"
              @click="showCustomUrlModal = false; openInApp(p.url)"
              class="px-3 py-2 bg-[#f8fafd] hover:bg-[#e8f0fe] hover:text-[#1a73e8] border border-[#dadce0]/60 rounded-xl text-xs font-medium text-[#3c4043] flex items-center justify-between transition-colors text-left"
            >
              <span>{{ p.name }}</span>
              <ExternalLink class="w-3 h-3 text-[#5f6368]" />
            </button>
          </div>
        </div>

        <!-- Custom URL Input -->
        <form @submit.prevent="handleOpenCustomUrl" class="space-y-4">
          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">输入任意网址</label>
            <input
              type="text"
              v-model="customUrlInput"
              placeholder="https://..."
              class="w-full px-3 py-2.5 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-mono"
            />
          </div>

          <div class="flex items-center justify-end gap-2 pt-2 border-t border-[#f1f3f4]">
            <button
              type="button"
              @click="showCustomUrlModal = false"
              class="px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#444746] rounded-xl text-xs font-medium"
            >
              取消
            </button>
            <button
              type="submit"
              class="px-5 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium shadow-sm flex items-center gap-1.5"
            >
              <ExternalLink class="w-3.5 h-3.5" />
              <span>在独立窗口打开</span>
            </button>
          </div>
        </form>
      </div>
    </div>

    <!-- 5. BACKUP CODES MANAGEMENT MODAL -->
    <div
      v-if="showBackupCodesModal && selectedRecord"
      class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="w-full max-w-lg bg-white rounded-3xl p-6 shadow-xl border border-[#dadce0] flex flex-col max-h-[85vh]">
        <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4] mb-3">
          <div>
            <h2 class="text-base font-bold text-[#202124]">备用验证码管理</h2>
            <p class="text-xs text-[#5f6368] mt-0.5">账号：<span class="font-medium text-[#1a73e8]">{{ selectedRecord.email }}</span></p>
          </div>
          <button @click="showBackupCodesModal = false" class="p-1.5 hover:bg-[#f1f3f4] rounded-full text-[#5f6368]">
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Mode Toggle & Action Bar -->
        <div class="flex items-center justify-between mb-4">
          <div class="text-xs text-[#5f6368]">
            共 <span class="font-bold text-[#202124]">{{ parsedBackupCodesList.length }}</span> 个代码，
            剩余 <span class="font-bold text-emerald-600">{{ unusedBackupCodesCount }}</span> 个可用
          </div>
          <div class="flex items-center gap-2">
            <button
              v-if="!isEditingBackupCodesRaw"
              @click="isEditingBackupCodesRaw = true"
              class="px-2.5 py-1 text-xs bg-[#f1f3f4] hover:bg-[#e8eaed] rounded-lg text-[#3c4043] transition-colors"
            >
              编辑 / 批量替换
            </button>
            <button
              v-else
              @click="isEditingBackupCodesRaw = false"
              class="px-2.5 py-1 text-xs bg-[#f1f3f4] hover:bg-[#e8eaed] rounded-lg text-[#3c4043] transition-colors"
            >
              返回卡片视图
            </button>
            <button
              @click="resetAllUsedCodes"
              class="px-2.5 py-1 text-xs text-rose-600 hover:bg-rose-50 rounded-lg transition-colors"
            >
              重置全部未用
            </button>
          </div>
        </div>

        <!-- Raw Text Editor View -->
        <div v-if="isEditingBackupCodesRaw" class="space-y-3 flex-1 overflow-y-auto">
          <p class="text-xs text-[#80868b]">请直接粘贴 Google 安全页面给出的 8 位备用码（每行一个或空格分隔均可）：</p>
          <textarea
            v-model="backupCodesRawInput"
            rows="8"
            placeholder="1234 5678&#10;8765 4321..."
            class="w-full p-3 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] font-mono text-xs outline-none resize-none leading-relaxed"
          ></textarea>
          <div class="flex justify-end gap-2">
            <button
              @click="isEditingBackupCodesRaw = false"
              class="px-4 py-2 bg-[#f1f3f4] text-[#444746] rounded-xl text-xs font-medium"
            >
              取消
            </button>
            <button
              @click="saveRawBackupCodes"
              class="px-5 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium shadow-sm"
            >
              保存备用码
            </button>
          </div>
        </div>

        <!-- Visual Interactive Grid View -->
        <div v-else class="flex-1 overflow-y-auto pr-1 space-y-4">
          <!-- Primary Fast Action -->
          <button
            @click="copyAndConsumeNextBackupCode"
            :disabled="unusedBackupCodesCount === 0"
            class="w-full py-3 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-2 shadow-sm transition-all disabled:opacity-50"
          >
            <Check v-if="copiedField === 'backup_code_consumed'" class="w-4 h-4" />
            <Copy v-else class="w-4 h-4" />
            <span>{{ copiedField === 'backup_code_consumed' ? '已复制下一个可用码并核销！' : '⚡ 复制下一个未用码并自动核销' }}</span>
          </button>

          <!-- Grid of codes -->
          <div v-if="parsedBackupCodesList.length > 0" class="grid grid-cols-2 gap-2">
            <div
              v-for="code in parsedBackupCodesList"
              :key="code"
              :class="[
                'p-3 rounded-xl border flex items-center justify-between transition-all select-text',
                usedBackupCodesSet.has(code)
                  ? 'bg-[#f1f3f4]/70 border-[#dadce0]/50 text-[#80868b]'
                  : 'bg-white border-[#dadce0] hover:border-[#1a73e8] text-[#202124]'
              ]"
            >
              <div class="flex items-center gap-2">
                <input
                  type="checkbox"
                  :checked="usedBackupCodesSet.has(code)"
                  @change="toggleBackupCode(code)"
                  class="rounded text-[#1a73e8] focus:ring-0 cursor-pointer"
                  title="标记为已使用 / 未使用"
                />
                <span
                  :class="[
                    'font-mono text-xs font-semibold tracking-wider',
                    usedBackupCodesSet.has(code) ? 'line-through opacity-60' : ''
                  ]"
                >
                  {{ code.slice(0, 4) }} {{ code.slice(4) }}
                </span>
              </div>
              <button
                @click="copyText(code, 'bc_' + code)"
                class="p-1 hover:bg-[#f1f3f4] rounded text-[#5f6368] transition-colors"
                title="复制此备用码"
              >
                <Check v-if="copiedField === 'bc_' + code" class="w-3.5 h-3.5 text-emerald-600" />
                <Copy v-else class="w-3.5 h-3.5" />
              </button>
            </div>
          </div>

          <div v-else class="py-8 text-center text-xs text-[#80868b]">
            暂无备用码，请点击右上角「编辑 / 批量替换」进行粘贴录入
          </div>
        </div>
      </div>
    </div>

    <!-- 6. BATCH IMPORT MODAL -->
    <div
      v-if="showBatchImportModal"
      class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="w-full max-w-2xl bg-white rounded-3xl p-6 shadow-xl border border-[#dadce0] flex flex-col max-h-[90vh]">
        <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4] mb-3">
          <div>
            <h2 class="text-base font-bold text-[#202124]">批量导入 Google 账号</h2>
            <p class="text-xs text-[#5f6368] mt-0.5">支持智能自动识别多格式（----、Tab制表符、逗号、空格分隔等）</p>
          </div>
          <button @click="showBatchImportModal = false" class="p-1.5 hover:bg-[#f1f3f4] rounded-full text-[#5f6368]">
            <X class="w-4 h-4" />
          </button>
        </div>

        <div class="space-y-4 flex-1 overflow-y-auto pr-1">
          <div>
            <div class="flex items-center justify-between mb-1">
              <label class="block text-xs font-semibold text-[#5f6368]">粘贴多行文本数据</label>
              <span class="text-[11px] text-[#80868b]">例: 邮箱----密码----辅助邮箱----TOTP密钥----备用码</span>
            </div>
            <textarea
              v-model="batchRawText"
              @input="handleBatchParse"
              rows="5"
              placeholder="user1@gmail.com----Password123----backup@outlook.com----JBSWY3DPEHPK3PXP&#10;user2@gmail.com    Password456"
              class="w-full p-3 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] font-mono text-xs outline-none resize-none leading-relaxed"
            ></textarea>
          </div>

          <div class="flex items-center gap-3">
            <div class="flex-1">
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">统一分配分组（可选）</label>
              <input
                type="text"
                v-model="batchAssignGroup"
                @input="handleBatchParse"
                placeholder="工作 / 个人 / 批量2026..."
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              />
            </div>
            <div class="text-right pt-4 text-xs font-medium text-[#1a73e8]">
              已智能识别：<span class="text-base font-bold">{{ batchParsedRecords.length }}</span> 个账号
            </div>
          </div>

          <!-- Preview Table -->
          <div v-if="batchParsedRecords.length > 0" class="border border-[#dadce0] rounded-2xl overflow-hidden max-h-56 overflow-y-auto">
            <table class="w-full text-left text-xs divide-y divide-[#f1f3f4]">
              <thead class="bg-[#f8fafd] text-[#5f6368] sticky top-0">
                <tr>
                  <th class="px-3 py-2 font-medium">#</th>
                  <th class="px-3 py-2 font-medium">邮箱</th>
                  <th class="px-3 py-2 font-medium">密码</th>
                  <th class="px-3 py-2 font-medium">辅助邮箱</th>
                  <th class="px-3 py-2 font-medium">TOTP 2FA</th>
                  <th class="px-3 py-2 font-medium">备用码</th>
                </tr>
              </thead>
              <tbody class="divide-y divide-[#f1f3f4] bg-white font-mono">
                <tr v-for="(r, idx) in batchParsedRecords" :key="idx" class="hover:bg-[#f8fafd]">
                  <td class="px-3 py-1.5 text-[#80868b]">{{ idx + 1 }}</td>
                  <td class="px-3 py-1.5 font-sans font-medium text-[#202124]">{{ r.email }}</td>
                  <td class="px-3 py-1.5 text-[#5f6368]">{{ r.password ? '••••••••' : '—' }}</td>
                  <td class="px-3 py-1.5 text-[#5f6368] truncate max-w-[120px]">{{ r.recovery_email || '—' }}</td>
                  <td class="px-3 py-1.5 text-[#1a73e8]">{{ r.totp ? '已识别' : '—' }}</td>
                  <td class="px-3 py-1.5 text-[#5f6368]">{{ r.backup_codes ? '有备用码' : '—' }}</td>
                </tr>
              </tbody>
            </table>
          </div>
        </div>

        <div class="flex items-center justify-end gap-2 pt-3 border-t border-[#f1f3f4] mt-3">
          <button
            type="button"
            @click="showBatchImportModal = false"
            class="px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#444746] rounded-xl text-xs font-medium"
          >
            取消
          </button>
          <button
            type="button"
            :disabled="batchParsedRecords.length === 0"
            @click="confirmBatchImport"
            class="px-5 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium shadow-sm disabled:opacity-50 flex items-center gap-1.5"
          >
            <Upload class="w-3.5 h-3.5" />
            <span>确认导入 {{ batchParsedRecords.length }} 个账号至保险库</span>
          </button>
        </div>
      </div>
    </div>

    <!-- 7. UNIFIED SETTINGS MODAL -->
    <div
      v-if="showSettingsModal"
      class="fixed inset-0 bg-black/45 backdrop-blur-sm z-50 flex items-center justify-center p-2 sm:p-4 animate-fade-in"
    >
      <div class="w-full max-w-2xl bg-white rounded-3xl shadow-2xl border border-[#dadce0] flex flex-col max-h-[92vh] sm:max-h-[88vh] overflow-hidden my-auto">
        
        <!-- Modal Top Bar -->
        <div class="px-4 sm:px-6 py-3 sm:py-4 border-b border-[#f1f3f4] flex items-center justify-between flex-shrink-0 bg-[#f8fafd]">
          <div class="flex items-center gap-2.5 min-w-0">
            <div class="w-8 h-8 rounded-xl bg-white border border-[#dadce0]/70 p-1 flex items-center justify-center shadow-2xs flex-shrink-0">
              <img src="/app.ico" alt="Logo" class="w-full h-full object-contain" />
            </div>
            <div class="min-w-0">
              <h2 class="text-base font-bold text-[#202124] truncate">设置与偏好</h2>
              <p class="text-[11px] text-[#5f6368] truncate">
                {{ isMobilePlatform ? '安全验证 · 云端端到端同步 · 备份导出 · 检查更新' : '设备安全解锁 · 云端端到端同步 · 备份导出 · 检查更新' }}
              </p>
            </div>
          </div>
          <button @click="showSettingsModal = false" class="p-1.5 hover:bg-[#e8eaed] rounded-full text-[#5f6368] transition-colors cursor-pointer flex-shrink-0">
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Navigation Segmented Tabs -->
        <div class="flex items-center gap-1.5 px-3 sm:px-6 py-2 sm:py-2.5 border-b border-[#f1f3f4] bg-white flex-shrink-0 text-xs font-medium overflow-x-auto no-scrollbar whitespace-nowrap">
          <button
            @click="settingsTab = 'security'"
            :class="[
              'px-3.5 py-1.5 rounded-full flex items-center gap-1.5 transition-all cursor-pointer whitespace-nowrap flex-shrink-0',
              settingsTab === 'security'
                ? 'bg-[#1a73e8] text-white font-semibold shadow-xs'
                : 'text-[#5f6368] hover:bg-[#f1f3f4] hover:text-[#202124]'
            ]"
          >
            <Fingerprint class="w-3.5 h-3.5 flex-shrink-0" />
            <span>{{ isMobilePlatform ? '安全验证' : '安全与解锁' }}</span>
          </button>

          <button
            @click="settingsTab = 'webdav'; loadWebDavConfig()"
            :class="[
              'px-3.5 py-1.5 rounded-full flex items-center gap-1.5 transition-all cursor-pointer whitespace-nowrap flex-shrink-0',
              settingsTab === 'webdav'
                ? 'bg-[#1a73e8] text-white font-semibold shadow-xs'
                : 'text-[#5f6368] hover:bg-[#f1f3f4] hover:text-[#202124]'
            ]"
          >
            <Cloud class="w-3.5 h-3.5 flex-shrink-0" />
            <span>云端同步</span>
          </button>

          <button
            @click="settingsTab = 'backups'; loadBackupsList()"
            :class="[
              'px-3.5 py-1.5 rounded-full flex items-center gap-1.5 transition-all cursor-pointer whitespace-nowrap flex-shrink-0',
              settingsTab === 'backups'
                ? 'bg-[#1a73e8] text-white font-semibold shadow-xs'
                : 'text-[#5f6368] hover:bg-[#f1f3f4] hover:text-[#202124]'
            ]"
          >
            <History class="w-3.5 h-3.5 flex-shrink-0" />
            <span>备份与导出</span>
          </button>

          <button
            @click="settingsTab = 'updates'"
            :class="[
              'px-3.5 py-1.5 rounded-full flex items-center gap-1.5 transition-all cursor-pointer whitespace-nowrap flex-shrink-0',
              settingsTab === 'updates'
                ? 'bg-[#1a73e8] text-white font-semibold shadow-xs'
                : 'text-[#5f6368] hover:bg-[#f1f3f4] hover:text-[#202124]'
            ]"
          >
            <RefreshCw class="w-3.5 h-3.5 flex-shrink-0" />
            <span>检查更新</span>
            <span v-if="updateResult?.has_update" class="w-2 h-2 rounded-full bg-amber-500 flex-shrink-0"></span>
          </button>

          <button
            @click="settingsTab = 'about'"
            :class="[
              'px-3.5 py-1.5 rounded-full flex items-center gap-1.5 transition-all cursor-pointer whitespace-nowrap flex-shrink-0',
              settingsTab === 'about'
                ? 'bg-[#1a73e8] text-white font-semibold shadow-xs'
                : 'text-[#5f6368] hover:bg-[#f1f3f4] hover:text-[#202124]'
            ]"
          >
            <Info class="w-3.5 h-3.5 flex-shrink-0" />
            <span>关于</span>
          </button>
        </div>

        <!-- Tab Body Content -->
        <div class="flex-1 overflow-y-auto p-3.5 sm:p-5 md:p-6 space-y-4">
          
          <!-- TAB 1: SECURITY & UNLOCK -->
          <div v-if="settingsTab === 'security'" class="space-y-4">
            <!-- Hardware Security Card -->
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-3">
              <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <div class="w-10 h-10 rounded-xl bg-white border border-[#dadce0]/70 flex items-center justify-center text-[#1a73e8] shadow-xs flex-shrink-0">
                    <Fingerprint class="w-5 h-5" />
                  </div>
                  <div class="min-w-0">
                    <h3 class="text-xs font-bold text-[#202124] flex items-center gap-2 flex-wrap">
                      <span>{{ isMobilePlatform ? '手机生物识别 / 屏幕锁快速解锁' : 'Windows Hello / PIN 生物识别解锁' }}</span>
                      <span
                        v-if="helloStatus.enrolled"
                        class="px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 text-[10px] font-semibold whitespace-nowrap"
                      >
                        已启用
                      </span>
                      <span
                        v-else-if="helloStatus.available"
                        class="px-2 py-0.5 rounded-full bg-gray-100 text-gray-600 text-[10px] font-semibold whitespace-nowrap"
                      >
                        {{ isMobilePlatform ? '手机支持 (未开启)' : '设备支持 (未开启)' }}
                      </span>
                      <span
                        v-else
                        class="px-2 py-0.5 rounded-full bg-amber-50 text-amber-700 text-[10px] font-semibold whitespace-nowrap"
                      >
                        系统不支持
                      </span>
                    </h3>
                    <p class="text-[11px] text-[#5f6368] mt-0.5 leading-relaxed">
                      {{ isMobilePlatform ? '开启后，每次打开软件可直接使用手机指纹、面容或锁屏密码完成一键安全解锁，无需输入主密码。' : '开启后，每次打开软件可直接使用当前电脑的指纹、面容或 PIN 码完成一秒安全解锁，无需反复输入主密码。' }}
                    </p>
                  </div>
                </div>

                <!-- Action Button -->
                <div v-if="helloStatus.available" class="flex-shrink-0">
                  <button
                    v-if="helloStatus.enrolled"
                    @click="toggleWindowsHello"
                    class="w-full sm:w-auto px-3.5 py-1.5 bg-white hover:bg-rose-50 text-rose-600 border border-rose-200 hover:border-rose-300 rounded-xl text-xs font-medium transition-all shadow-xs cursor-pointer text-center"
                  >
                    {{ isMobilePlatform ? '停用手机验证' : '停用免密解锁' }}
                  </button>
                  <button
                    v-else
                    @click="toggleWindowsHello"
                    class="w-full sm:w-auto px-4 py-1.5 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium transition-all shadow-xs flex items-center justify-center gap-1.5 cursor-pointer"
                  >
                    <Fingerprint class="w-3.5 h-3.5" />
                    <span>{{ isMobilePlatform ? '立即绑定手机验证' : '立即绑定设备' }}</span>
                  </button>
                </div>
              </div>

              <!-- Tech Security Note -->
              <div class="p-3 bg-white/80 rounded-xl border border-[#dadce0]/60 text-[11px] text-[#5f6368] space-y-1">
                <div class="font-semibold text-[#202124] flex items-center gap-1.5">
                  <CheckCircle2 class="w-3.5 h-3.5 text-emerald-600 flex-shrink-0" />
                  <span>硬件级安全机制保护</span>
                </div>
                <p class="leading-relaxed">
                  {{ isMobilePlatform ? '在移动端受 Android Keystore 硬件级密钥安全机制与 BiometricPrompt 生物识别框架保护。未通过设备授权验证无法解密任何密码凭据。' : '绑定凭据通过微软 Win32 DPAPI 与当前 Windows 用户安全上下文硬件绑定，并通过 WinRT Interop 保证弹窗直接置顶。即使密码库文件被复制到其他电脑，也绝对无法通过非授权设备解锁。' }}
                </p>
              </div>
            </div>

            <!-- Privacy & Selection Preferences -->
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-3">
              <div class="flex items-center justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <div class="w-10 h-10 rounded-xl bg-white border border-[#dadce0]/70 flex items-center justify-center text-[#1a73e8] shadow-xs flex-shrink-0">
                    <Shield class="w-5 h-5" />
                  </div>
                  <div class="min-w-0">
                    <h3 class="text-xs font-bold text-[#202124] flex items-center gap-2">
                      <span>解锁后自动选中首个账号</span>
                      <span
                        v-if="!autoSelectFirst"
                        class="px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-800 text-[10px] font-semibold whitespace-nowrap"
                      >
                        已启用防窥保护
                      </span>
                    </h3>
                    <p class="text-[11px] text-[#5f6368] mt-0.5">
                      {{ autoSelectFirst ? '已开启：解锁后将自动高亮首个账号并展示其详情与 2FA 动态码。' : '推荐关闭（已生效）：解锁后保持留白防窥状态，点击列表才展开详情。' }}
                    </p>
                  </div>
                </div>

                <button
                  type="button"
                  @click="toggleAutoSelectFirst"
                  :class="[
                    'relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none',
                    autoSelectFirst ? 'bg-[#1a73e8]' : 'bg-[#dadce0]'
                  ]"
                >
                  <span
                    :class="[
                      'pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out',
                      autoSelectFirst ? 'translate-x-5' : 'translate-x-0'
                    ]"
                  />
                </button>
              </div>

              <!-- Shortcut Tips Banner -->
              <div class="p-3 bg-white/80 rounded-xl border border-[#dadce0]/60 text-[11px] text-[#5f6368] space-y-1">
                <div class="font-semibold text-[#202124] flex items-center gap-1.5">
                  <Sparkles class="w-3.5 h-3.5 text-[#1a73e8] flex-shrink-0" />
                  <span>防窥与反选操作技巧</span>
                </div>
                <p class="leading-relaxed text-[11px]">
                  • <b>再次点击已选账号</b>：直接取消选中并关闭详情。<br />
                  • <b>按 Esc 键或点击空白区域</b>：快速返回安全留白防窥状态。<br />
                  • <b>详情右上角关闭按钮</b>：一键折叠当前账号。
                </p>
              </div>
            </div>

            <!-- Master Password Encryption Architecture -->
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-2">
              <h3 class="text-xs font-bold text-[#202124] flex items-center gap-2">
                <Lock class="w-4 h-4 text-[#1a73e8] flex-shrink-0" />
                <span>主密码与本地加密体系</span>
              </h3>
              <p class="text-[11px] text-[#5f6368] leading-relaxed">
                当前安全库所有账号数据使用 <b>AES-256-GCM</b> 强加密，密钥派生采用 <b>Scrypt (N=32768, r=8, p=1)</b> 算法。数据绝无任何明文落盘，主密码本身也不存储于任何文件。请务必牢记主密码。
              </p>
            </div>
          </div>

          <!-- TAB 2: WEBDAV CLOUD SYNC -->
          <div v-else-if="settingsTab === 'webdav'" class="space-y-4">
            <!-- 1. Full-Auto Smart Sync Engine Control -->
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-3">
              <div class="flex items-center justify-between gap-2 pb-2 border-b border-[#dadce0]/60">
                <div class="flex items-center gap-2">
                  <Zap class="w-4 h-4 text-[#1a73e8] flex-shrink-0" />
                  <span class="text-xs font-bold text-[#202124]">全自动增量智能同步 (Zero-Touch & Smart Merge)</span>
                </div>
                <!-- Auto Sync Toggle Switch -->
                <button
                  type="button"
                  @click="webdavForm.auto_sync = !webdavForm.auto_sync; handleSaveWebDavConfig(false)"
                  :class="[
                    'relative inline-flex h-6 w-11 flex-shrink-0 cursor-pointer rounded-full border-2 border-transparent transition-colors duration-200 ease-in-out focus:outline-none',
                    webdavForm.auto_sync ? 'bg-[#1a73e8]' : 'bg-[#dadce0]'
                  ]"
                  role="switch"
                  :aria-checked="webdavForm.auto_sync"
                >
                  <span
                    :class="[
                      'pointer-events-none inline-block h-5 w-5 transform rounded-full bg-white shadow ring-0 transition duration-200 ease-in-out',
                      webdavForm.auto_sync ? 'translate-x-5' : 'translate-x-0'
                    ]"
                  />
                </button>
              </div>

              <p class="text-[11px] text-[#5f6368] leading-relaxed">
                启用后，本地<b>新增、修改、核销备用码或删除账号</b>将在 <b>1.5 秒</b>内自动静默推送到云端；启动应用或前台唤醒时利用轻量 <b>ETag</b> 极速拉取增量；通过毫秒级修改时间戳与逻辑删除墓碑进行冲突仲裁，<b>多设备互相同步零覆盖风险</b>。
              </p>

              <!-- Sync Status Summary Grid -->
              <div class="grid grid-cols-2 sm:grid-cols-3 gap-2 py-1">
                <div class="p-2.5 bg-white rounded-xl border border-[#dadce0]/70 flex flex-col gap-0.5">
                  <span class="text-[10px] text-[#80868b] font-medium">上次同步时间</span>
                  <span class="text-xs font-semibold text-[#202124] truncate">{{ lastSyncTimeFormatted }}</span>
                </div>
                <div class="p-2.5 bg-white rounded-xl border border-[#dadce0]/70 flex flex-col gap-0.5">
                  <span class="text-[10px] text-[#80868b] font-medium">同步引擎状态</span>
                  <div class="flex items-center gap-1.5">
                    <span
                      :class="[
                        'w-1.5 h-1.5 rounded-full flex-shrink-0',
                        syncState === 'syncing' ? 'bg-[#1a73e8] animate-ping' :
                        syncState === 'error' ? 'bg-rose-500' :
                        webdavForm.auto_sync ? 'bg-emerald-500' : 'bg-gray-400'
                      ]"
                    ></span>
                    <span class="text-xs font-semibold text-[#202124] truncate">
                      {{ syncState === 'syncing' ? '正在同步...' : syncState === 'error' ? '同步异常' : webdavForm.auto_sync ? '全自动监控中' : '自动同步已暂停' }}
                    </span>
                  </div>
                </div>
                <div class="col-span-2 sm:col-span-1 p-2.5 bg-white rounded-xl border border-[#dadce0]/70 flex flex-col gap-0.5">
                  <span class="text-[10px] text-[#80868b] font-medium">云端文件版本</span>
                  <span class="text-xs font-mono font-medium text-[#5f6368] truncate" :title="webdavForm.last_etag || '未获取'">
                    {{ webdavForm.last_etag ? webdavForm.last_etag.slice(0, 12) + '...' : '尚未生成' }}
                  </span>
                </div>
              </div>

              <!-- Manual Trigger Smart Sync Button -->
              <div class="pt-1">
                <button
                  type="button"
                  @click="triggerSmartSync(false)"
                  :disabled="isWebDavSyncing"
                  class="w-full py-2.5 px-3 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-2 shadow-sm transition-all disabled:opacity-50 cursor-pointer"
                >
                  <RefreshCw :class="['w-3.5 h-3.5', isWebDavSyncing ? 'animate-spin' : '']" />
                  <span>⚡ 立即执行全自动增量智能合并</span>
                </button>
              </div>
            </div>

            <!-- 2. Server Connection Config -->
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-3">
              <div class="flex flex-col sm:flex-row sm:items-center gap-1.5 pb-2 border-b border-[#dadce0]/60">
                <div class="flex items-center gap-2">
                  <Cloud class="w-4 h-4 text-[#1a73e8] flex-shrink-0" />
                  <span class="text-xs font-bold text-[#202124]">WebDAV 服务器连接配置</span>
                </div>
                <span class="text-[10px] text-[#5f6368]">（支持坚果云、Nextcloud、群晖等标准网盘）</span>
              </div>

              <div class="space-y-3">
                <div>
                  <label class="block text-xs font-semibold text-[#5f6368] mb-1">WebDAV 服务器地址</label>
                  <input
                    type="text"
                    v-model="webdavForm.url"
                    @change="handleSaveWebDavConfig(false)"
                    placeholder="https://dav.jianguoyun.com/dav/GoogleVault/"
                    class="w-full px-3 py-2 bg-white rounded-xl border border-[#dadce0] focus:border-[#1a73e8] text-xs outline-none font-mono"
                  />
                </div>

                <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                  <div>
                    <label class="block text-xs font-semibold text-[#5f6368] mb-1">账号 / 邮箱</label>
                    <input
                      type="text"
                      v-model="webdavForm.username"
                      @change="handleSaveWebDavConfig(false)"
                      placeholder="your_email@example.com"
                      class="w-full px-3 py-2 bg-white rounded-xl border border-[#dadce0] focus:border-[#1a73e8] text-xs outline-none"
                    />
                  </div>
                  <div>
                    <label class="block text-xs font-semibold text-[#5f6368] mb-1">应用独立密码 / Token</label>
                    <input
                      type="password"
                      v-model="webdavForm.password"
                      @change="handleSaveWebDavConfig(false)"
                      placeholder="专用应用授权密码..."
                      class="w-full px-3 py-2 bg-white rounded-xl border border-[#dadce0] focus:border-[#1a73e8] text-xs outline-none font-mono"
                    />
                  </div>
                </div>

                <div v-if="webdavStatus" class="p-2.5 bg-white rounded-xl border border-[#dadce0] text-xs leading-relaxed text-[#3c4043]">
                  {{ webdavStatus }}
                </div>

                <!-- Test & Save buttons -->
                <div class="flex flex-wrap items-center gap-2 pt-1">
                  <button
                    type="button"
                    @click="handleTestWebDav"
                    :disabled="isWebDavSyncing"
                    class="px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium flex items-center gap-1.5 transition-colors disabled:opacity-50 cursor-pointer"
                  >
                    <RefreshCw :class="['w-3.5 h-3.5 text-[#1a73e8]', isWebDavSyncing ? 'animate-spin' : '']" />
                    <span>测试连接</span>
                  </button>
                  <button
                    type="button"
                    @click="handleSaveWebDavConfig(true)"
                    class="px-4 py-2 bg-white hover:bg-[#f1f3f4] text-[#3c4043] border border-[#dadce0] rounded-xl text-xs font-medium transition-colors cursor-pointer"
                  >
                    保存配置
                  </button>
                </div>
              </div>
            </div>

            <!-- 3. Manual Cold-Backup / Fallback Actions -->
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-3">
              <h3 class="text-xs font-bold text-[#202124]">手动冷备份与恢复覆盖（备用降级操作）</h3>
              <div class="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <button
                  type="button"
                  @click="handleUploadWebDav"
                  :disabled="isWebDavSyncing"
                  class="py-2.5 px-3 bg-white hover:bg-[#f1f3f4] text-[#3c4043] border border-[#dadce0] rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-all disabled:opacity-50 cursor-pointer"
                >
                  <Upload class="w-3.5 h-3.5 flex-shrink-0" />
                  <span>⬆ 全库强制覆盖上传至云端</span>
                </button>
                <button
                  type="button"
                  @click="handleDownloadWebDav"
                  :disabled="isWebDavSyncing"
                  class="py-2.5 px-3 bg-white hover:bg-[#f1f3f4] text-[#3c4043] border border-[#dadce0] rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-all disabled:opacity-50 cursor-pointer"
                >
                  <Download class="w-3.5 h-3.5 flex-shrink-0" />
                  <span>⬇ 从云端下载强制覆盖本地</span>
                </button>
              </div>
              <p class="text-[11px] text-[#80868b] leading-relaxed">
                提示：正常情况下使用上方的「全自动增量智能同步」即可；此处的强制覆盖操作用于整库灾难恢复或冷初始化。执行强制覆盖前系统均会自动为本地数据创建安全快照。
              </p>
            </div>
          </div>

          <!-- TAB 3: BACKUPS & EXPORT -->
          <div v-else-if="settingsTab === 'backups'" class="space-y-4">
            <!-- Sub-tabs -->
            <div class="flex gap-2 p-1 bg-[#f1f3f4] rounded-xl text-xs font-medium">
              <button
                @click="backupTab = 'backups'"
                :class="[
                  'flex-1 py-1.5 px-2 rounded-lg transition-all cursor-pointer text-center truncate',
                  backupTab === 'backups' ? 'bg-white text-[#1a73e8] shadow-sm font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
                ]"
              >
                历史快照回滚 ({{ backupList.length }})
              </button>
              <button
                @click="backupTab = 'export'"
                :class="[
                  'flex-1 py-1.5 px-2 rounded-lg transition-all cursor-pointer text-center truncate',
                  backupTab === 'export' ? 'bg-white text-[#1a73e8] shadow-sm font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
                ]"
              >
                数据导出与密文备份
              </button>
            </div>

            <!-- Sub-Tab 1: Backups List -->
            <div v-if="backupTab === 'backups'" class="space-y-2 max-h-72 overflow-y-auto pr-1">
              <div v-if="isLoadingBackups" class="py-8 text-center text-xs text-[#80868b]">
                正在检索本地快照备份目录...
              </div>
              <div v-else-if="backupList.length === 0" class="py-8 text-center text-xs text-[#80868b]">
                暂无历史快照备份（每次编辑或导入账号时会自动生成快照备份）
              </div>
              <div
                v-else
                v-for="b in backupList"
                :key="b.filename"
                class="p-3 bg-[#f8fafd] hover:bg-white border border-[#dadce0]/70 rounded-xl flex flex-col sm:flex-row sm:items-center justify-between gap-2 transition-colors"
              >
                <div class="min-w-0">
                  <div class="font-mono text-xs font-semibold text-[#202124]">{{ b.timestamp }}</div>
                  <div class="text-[11px] text-[#80868b] mt-0.5 font-mono truncate">
                    {{ b.filename }} ({{ (b.size_bytes / 1024).toFixed(1) }} KB)
                  </div>
                </div>
                <button
                  @click="handleRestoreBackup(b.filename)"
                  class="w-full sm:w-auto px-3 py-1.5 bg-[#e8f0fe] hover:bg-[#d2e3fc] text-[#1a73e8] rounded-lg text-xs font-medium transition-colors cursor-pointer text-center whitespace-nowrap"
                >
                  回滚此版本
                </button>
              </div>
            </div>

            <!-- Sub-Tab 2: Export Options -->
            <div v-else class="space-y-3 py-1">
              <!-- Encrypted .gvault Card (Recommended) -->
              <div class="p-4 bg-gradient-to-br from-emerald-50 via-teal-50/30 to-blue-50/40 rounded-2xl border border-emerald-200/90 flex flex-col sm:flex-row sm:items-center justify-between gap-3 shadow-2xs">
                <div class="space-y-1">
                  <div class="flex items-center gap-2">
                    <span class="p-1 rounded-lg bg-emerald-100 text-emerald-800">
                      <Lock class="w-3.5 h-3.5" />
                    </span>
                    <h3 class="text-xs font-bold text-emerald-950 flex items-center gap-1.5">
                      <span>导出密文保险库文件 (.gvault)</span>
                      <span class="px-2 py-0.2 rounded-full bg-emerald-200/70 text-emerald-800 text-[9px] font-semibold">强烈推荐</span>
                    </h3>
                  </div>
                  <p class="text-[11px] text-emerald-800/85 leading-relaxed max-w-xl">
                    采用 <strong>AES-256-GCM + Scrypt 军事级强加密</strong>，所有账号与 2FA 密匙以高强密文封装。可在任意设备上直接导入无损还原，安全防泄露。
                  </p>
                </div>
                <div class="flex items-center gap-2 flex-wrap sm:flex-nowrap flex-shrink-0">
                  <button
                    @click="triggerImportGvaultFile"
                    class="w-full sm:w-auto px-3.5 py-2 bg-white hover:bg-[#f1f3f4] text-emerald-900 border border-emerald-300 rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-colors cursor-pointer whitespace-nowrap shadow-2xs"
                    title="选择本地 .gvault 密文文件导入并解密合并"
                  >
                    <Upload class="w-3.5 h-3.5 text-emerald-700" />
                    <span>导入 .gvault</span>
                  </button>
                  <button
                    @click="handleExportGvault"
                    class="w-full sm:w-auto px-4 py-2 bg-emerald-600 hover:bg-emerald-700 active:bg-emerald-800 text-white rounded-xl text-xs font-semibold flex items-center justify-center gap-1.5 shadow-sm transition-all cursor-pointer whitespace-nowrap"
                  >
                    <Download class="w-3.5 h-3.5" />
                    <span>导出加密 .gvault</span>
                  </button>
                </div>
              </div>

              <!-- Plain CSV Card -->
              <div class="p-3.5 bg-[#f8fafd] rounded-2xl border border-[#dadce0] flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="space-y-1">
                  <h3 class="text-xs font-bold text-[#202124]">导出为标准 CSV 表格文件</h3>
                  <p class="text-[11px] text-[#5f6368] leading-relaxed">
                    包含邮箱、登录密码、账号状态、分组、辅助邮箱、TOTP密钥、备用验证码等全部明文字段，支持 Excel/Numbers 打开。
                  </p>
                </div>
                <button
                  @click="handleExportCsv"
                  class="w-full sm:w-auto px-4 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-sm transition-all cursor-pointer whitespace-nowrap flex-shrink-0"
                >
                  <FileText class="w-3.5 h-3.5" />
                  <span>导出明文 CSV</span>
                </button>
              </div>

              <!-- Plain JSON Card -->
              <div class="p-3.5 bg-[#f8fafd] rounded-2xl border border-[#dadce0] flex flex-col sm:flex-row sm:items-center justify-between gap-3">
                <div class="space-y-1">
                  <h3 class="text-xs font-bold text-[#202124]">导出为 JSON 数据格式</h3>
                  <p class="text-[11px] text-[#5f6368] leading-relaxed">
                    结构化明文备份，便于迁移至自建数据库、密码管理器或自行脚本解析。
                  </p>
                </div>
                <button
                  @click="handleExportJson"
                  class="w-full sm:w-auto px-4 py-2 bg-white hover:bg-[#f1f3f4] text-[#3c4043] border border-[#dadce0] rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-colors cursor-pointer whitespace-nowrap flex-shrink-0"
                >
                  <Download class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>导出明文 JSON</span>
                </button>
              </div>
            </div>
          </div>

          <!-- TAB 4: CHECK FOR UPDATES -->
          <div v-else-if="settingsTab === 'updates'" class="space-y-4">
            <div class="p-3.5 sm:p-4 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-3">
              <div class="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                  <div class="w-10 h-10 rounded-xl bg-white border border-[#dadce0]/70 flex items-center justify-center text-[#1a73e8] shadow-xs flex-shrink-0">
                    <RefreshCw class="w-5 h-5" />
                  </div>
                  <div class="min-w-0">
                    <h3 class="text-xs font-bold text-[#202124] flex items-center gap-2 flex-wrap">
                      <span>软件版本检查更新</span>
                      <span class="px-2 py-0.5 rounded-full bg-[#e8f0fe] text-[#1a73e8] text-[10px] font-mono font-bold whitespace-nowrap">
                        当前: v3.1.15
                      </span>
                    </h3>
                    <p class="text-[11px] text-[#5f6368] mt-0.5">
                      检测 GitHub Releases 官方版本更新，获取最新特性与安全修复
                    </p>
                  </div>
                </div>

                <button
                  @click="checkForUpdates"
                  :disabled="isCheckingUpdate || isDownloadingUpdate"
                  class="w-full sm:w-auto px-4 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-sm transition-all disabled:opacity-50 cursor-pointer whitespace-nowrap flex-shrink-0"
                >
                  <RefreshCw :class="['w-3.5 h-3.5', isCheckingUpdate ? 'animate-spin' : '']" />
                  <span>{{ isCheckingUpdate ? "正在检查..." : "立即检查更新" }}</span>
                </button>
              </div>

              <!-- Update Mirror Source Selector -->
              <div class="pt-2.5 border-t border-[#dadce0]/60 flex flex-col sm:flex-row sm:items-center justify-between gap-2.5">
                <div class="space-y-0.5">
                  <div class="flex items-center gap-1.5">
                    <span class="text-xs font-bold text-[#202124]">更新下载网络源</span>
                    <span v-if="updateMirror === 'gh-proxy'" class="px-1.5 py-0.2 rounded-md bg-emerald-100 text-emerald-800 text-[10px] font-semibold">推荐</span>
                  </div>
                  <p class="text-[11px] text-[#5f6368]">
                    {{
                      updateMirror === 'gh-proxy'
                        ? '已启用 gh-proxy.com 高速代理，快速访问 GitHub 文件与安装包（推荐）'
                        : updateMirror === 'ghproxy'
                        ? '已启用 ghproxy.net 镜像加速通道，提升国内下载与检测稳定性'
                        : '使用 GitHub 官方直连通道，海外网络或自建代理环境推荐'
                    }}
                  </p>
                </div>

                <div class="flex items-center gap-1.5 p-1 bg-[#f1f3f4] rounded-xl self-start sm:self-auto flex-shrink-0 flex-wrap">
                  <button
                    type="button"
                    @click="setUpdateMirror('gh-proxy')"
                    :class="[
                      'px-2.5 py-1.5 rounded-lg text-xs font-medium transition-all cursor-pointer flex items-center gap-1.5',
                      updateMirror === 'gh-proxy' ? 'bg-white text-emerald-700 shadow-2xs font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
                    ]"
                  >
                    <Zap class="w-3.5 h-3.5 text-emerald-600" />
                    <span>gh-proxy (推荐)</span>
                  </button>
                  <button
                    type="button"
                    @click="setUpdateMirror('ghproxy')"
                    :class="[
                      'px-2.5 py-1.5 rounded-lg text-xs font-medium transition-all cursor-pointer flex items-center gap-1.5',
                      updateMirror === 'ghproxy' ? 'bg-white text-emerald-700 shadow-2xs font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
                    ]"
                  >
                    <Zap class="w-3.5 h-3.5 text-emerald-600" />
                    <span>ghproxy.net</span>
                  </button>
                  <button
                    type="button"
                    @click="setUpdateMirror('official')"
                    :class="[
                      'px-2.5 py-1.5 rounded-lg text-xs font-medium transition-all cursor-pointer flex items-center gap-1.5',
                      updateMirror === 'official' ? 'bg-white text-[#1a73e8] shadow-2xs font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
                    ]"
                  >
                    <Globe class="w-3.5 h-3.5 text-[#1a73e8]" />
                    <span>官方直连</span>
                  </button>
                </div>
              </div>
            </div>

            <!-- Update Results Section -->
            <div v-if="updateResult" class="space-y-3">
              <!-- Case A: New Version Available -->
              <div
                v-if="updateResult.has_update"
                class="p-4 bg-gradient-to-br from-emerald-50 to-teal-50/40 border border-emerald-200 rounded-2xl space-y-3.5 shadow-xs"
              >
                <!-- Title & Version Info -->
                <div class="flex flex-col sm:flex-row sm:items-start justify-between gap-3">
                  <div class="flex items-start gap-2.5">
                    <div class="w-8 h-8 rounded-xl bg-emerald-100 flex items-center justify-center text-emerald-700 flex-shrink-0 mt-0.5">
                      <Sparkles class="w-4 h-4" />
                    </div>
                    <div>
                      <h4 class="text-sm font-bold text-emerald-950 flex items-center gap-2 flex-wrap">
                        <span>发现新版本：{{ updateResult.latest_version }}</span>
                        <span class="px-2 py-0.5 rounded-full bg-emerald-200/70 text-emerald-800 text-[10px] font-mono font-bold">
                          可升级
                        </span>
                        <span class="px-2 py-0.5 rounded-full bg-white text-emerald-800 border border-emerald-300 text-[10px] font-semibold shadow-2xs">
                          {{ currentInstallTypeName }}
                        </span>
                      </h4>
                      <p class="text-[11px] text-emerald-700 mt-0.5">
                        {{ updateResult.release_name }} · 发布于 {{ updateResult.published_at.substring(0, 10) }}
                      </p>
                    </div>
                  </div>

                  <!-- Primary Action: In-app auto-update directly for CURRENT version only -->
                  <div class="flex items-center gap-2 flex-wrap sm:flex-nowrap">
                    <button
                      v-if="!isDownloadingUpdate"
                      @click="startAutoUpdate()"
                      class="w-full sm:w-auto px-4 py-2 bg-emerald-600 hover:bg-emerald-700 active:bg-emerald-800 text-white rounded-xl text-xs font-semibold shadow-sm transition-all flex items-center justify-center gap-1.5 cursor-pointer whitespace-nowrap flex-shrink-0"
                    >
                      <RefreshCw class="w-3.5 h-3.5" />
                      <span>一键自动更新 (推荐)</span>
                    </button>

                    <button
                      v-if="!isDownloadingUpdate"
                      @click="openExternalUrl(updateResult.download_url || updateResult.release_url)"
                      class="w-full sm:w-auto px-3 py-2 bg-white hover:bg-emerald-50 text-emerald-800 border border-emerald-300 rounded-xl text-xs font-medium transition-all flex items-center justify-center gap-1.5 cursor-pointer whitespace-nowrap flex-shrink-0"
                      title="直接通过浏览器下载当前版本的安装包"
                    >
                      <Download class="w-3.5 h-3.5" />
                      <span>浏览器下载</span>
                    </button>
                  </div>
                </div>

                <!-- Active In-App Downloading Progress Bar -->
                <div v-if="isDownloadingUpdate" class="p-3 bg-white/90 rounded-xl border border-emerald-200/80 space-y-2">
                  <div class="flex items-center justify-between text-xs font-medium text-emerald-900">
                    <div class="flex items-center gap-2">
                      <RefreshCw class="w-3.5 h-3.5 text-emerald-600 animate-spin" />
                      <span>{{ downloadStatusText }}</span>
                    </div>
                    <span class="font-mono font-bold">{{ downloadProgress }}%</span>
                  </div>
                  <div class="w-full h-2.5 bg-emerald-100 rounded-full overflow-hidden">
                    <div
                      class="h-full bg-emerald-600 rounded-full transition-all duration-300 ease-out"
                      :style="{ width: `${downloadProgress}%` }"
                    ></div>
                  </div>
                  <p class="text-[10px] text-emerald-700">
                    {{ isMobilePlatform ? '下载完成后将自动调起系统包安装器进行无缝覆盖升级。' : '下载完成后将自动启动最新版安装包进行无缝升级并退出当前版本。' }}
                  </p>
                </div>

                <!-- Release Notes (Beautifully formatted) -->
                <div v-if="parsedReleaseNotes" class="p-3 bg-white/95 rounded-xl border border-emerald-100 text-xs shadow-2xs space-y-2">
                  <div class="flex items-center justify-between pb-1.5 border-b border-[#dadce0]/50 text-[11px] font-bold text-[#202124]">
                    <div class="flex items-center gap-1.5 text-emerald-900">
                      <span>{{ parsedReleaseNotes.title }}</span>
                    </div>
                    <span v-if="parsedReleaseNotes.items.length > 0" class="text-[10px] text-[#5f6368] font-normal font-mono">
                      共 {{ parsedReleaseNotes.items.length }} 项改动
                    </span>
                  </div>

                  <div v-if="parsedReleaseNotes.items.length > 0" class="space-y-1.5 max-h-52 overflow-y-auto pr-1">
                    <div
                      v-for="(item, idx) in parsedReleaseNotes.items"
                      :key="idx"
                      class="flex items-start gap-1.5 leading-relaxed text-[#202124] text-[11.5px]"
                    >
                      <span class="w-1.5 h-1.5 rounded-full bg-emerald-500 mt-1.5 flex-shrink-0"></span>
                      <div class="flex-1 min-w-0 flex items-baseline gap-1.5 flex-wrap sm:flex-nowrap">
                        <div class="flex items-center gap-1 flex-shrink-0">
                          <span
                            :class="[
                              'px-1.5 py-0.2 rounded border text-[10px] font-medium leading-tight whitespace-nowrap',
                              item.typeColor
                            ]"
                          >
                            {{ item.typeLabel }}
                          </span>
                          <span
                            v-if="item.scopeLabel"
                            class="px-1.5 py-0.2 rounded bg-[#f1f3f4] text-[#5f6368] text-[10px] font-medium leading-tight whitespace-nowrap"
                          >
                            {{ item.scopeLabel }}
                          </span>
                        </div>
                        <span class="text-[#3c4043] leading-normal break-words">{{ item.text }}</span>
                      </div>
                    </div>
                  </div>

                  <div
                    v-else-if="parsedReleaseNotes.rawText"
                    class="text-[#3c4043] whitespace-pre-wrap leading-relaxed max-h-48 overflow-y-auto text-[11px]"
                  >
                    {{ parsedReleaseNotes.rawText }}
                  </div>
                </div>
              </div>

              <!-- Case B: Already Latest -->
              <div
                v-else
                class="p-4 bg-white border border-[#dadce0] rounded-2xl flex items-center gap-3 text-xs text-[#202124]"
              >
                <CheckCircle2 class="w-5 h-5 text-emerald-600 flex-shrink-0" />
                <div>
                  <div class="font-bold">当前已是最新版本 ({{ updateResult.current_version }})</div>
                  <div class="text-[11px] text-[#5f6368] mt-0.5">您正在使用最新的 Google 账号管家，无需更新。</div>
                </div>
              </div>
            </div>

            <div v-else-if="updateError" class="p-4 bg-rose-50 border border-rose-200 rounded-2xl text-xs text-rose-800 space-y-1">
              <div class="font-bold flex items-center gap-1.5">
                <AlertCircle class="w-4 h-4 text-rose-600" />
                <span>检查更新失败</span>
              </div>
              <p class="text-[11px] text-rose-600 leading-relaxed">{{ updateError }}</p>
            </div>
          </div>

          <!-- TAB 5: ABOUT -->
          <div v-else-if="settingsTab === 'about'" class="space-y-4">
            <div class="p-5 bg-[#f8fafd] rounded-2xl border border-[#dadce0] flex flex-col items-center text-center space-y-3">
              <div class="w-16 h-16 rounded-2xl bg-white p-2.5 border border-[#dadce0] shadow-xs overflow-hidden flex items-center justify-center">
                <img src="/app.ico" alt="Logo" class="w-full h-full object-contain" />
              </div>
              <div>
                <h3 class="text-base font-bold text-[#202124]">Google 账号管家</h3>
                <p class="text-xs text-[#1a73e8] font-mono font-medium mt-0.5">v3.1.15</p>
              </div>
              <p class="text-xs text-[#5f6368] max-w-md leading-relaxed">
                面向专业运维与个人多账号管理的高性能轻量跨平台保险库，集成 RFC 6238 2FA 动态码、硬件级免密解锁、多隔离环境浏览器会话与 WebDAV 端到端纯密文同步。
              </p>

              <div class="flex items-center gap-2 pt-1">
                <button
                  @click="openExternalUrl('https://github.com/ifnor/GoogleAccountVault')"
                  class="px-3.5 py-1.5 bg-white hover:bg-[#f1f3f4] text-[#202124] border border-[#dadce0] rounded-xl text-xs font-medium flex items-center gap-1.5 transition-colors shadow-xs cursor-pointer"
                >
                  <Globe class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>GitHub 项目主页</span>
                </button>
              </div>
            </div>

            <div class="p-3 bg-[#f8fafd] rounded-xl border border-[#dadce0] text-xs">
              <div class="font-bold text-[#202124]">安全标准</div>
              <div class="text-[11px] text-[#5f6368] mt-1 font-mono">{{ isMobilePlatform ? 'AES-256-GCM · Scrypt · Keystore' : 'AES-256-GCM · Scrypt · DPAPI' }}</div>
            </div>
          </div>

        </div>

        <!-- Modal Bottom Close -->
        <div class="px-4 sm:px-6 py-3 border-t border-[#f1f3f4] bg-[#f8fafd] flex justify-end flex-shrink-0">
          <button
            @click="showSettingsModal = false"
            class="w-full sm:w-auto px-5 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium transition-colors shadow-xs cursor-pointer text-center"
          >
            完成
          </button>
        </div>

      </div>
    </div>



    <!-- MODAL 9: PASSWORD GENERATOR MODAL -->
    <div
      v-if="showPasswordGenModal"
      class="fixed inset-0 z-50 flex items-center justify-center p-4 bg-black/40 backdrop-blur-sm animate-fade-in"
    >
      <div class="w-full max-w-lg bg-white rounded-3xl p-6 shadow-2xl border border-[#dadce0] flex flex-col gap-5">
        <div class="flex items-center justify-between pb-3 border-b border-[#dadce0]">
          <div class="flex items-center gap-2.5">
            <div class="w-8 h-8 rounded-xl bg-[#e8f0fe] flex items-center justify-center text-[#1a73e8]">
              <KeyRound class="w-4 h-4" />
            </div>
            <div>
              <h2 class="text-sm font-bold text-[#202124]">强随机密码生成器</h2>
              <p class="text-[11px] text-[#5f6368]">自定义长度与多字符集加密生成高熵安全凭据</p>
            </div>
          </div>
          <button
            @click="showPasswordGenModal = false"
            class="text-[#5f6368] hover:text-[#202124] p-1.5 hover:bg-[#f1f3f4] rounded-full transition-colors"
          >
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Password Display Box -->
        <div class="bg-[#f8fafd] border border-[#dadce0] rounded-2xl p-4 flex flex-col gap-2.5">
          <div class="flex items-center justify-between gap-3">
            <span class="font-mono text-base md:text-lg font-bold text-[#1a73e8] tracking-wider break-all select-all flex-1">
              {{ generatedPassword || "生成中..." }}
            </span>
            <div class="flex items-center gap-1.5 flex-shrink-0">
              <button
                @click="generateNewPassword"
                class="p-2 hover:bg-[#e8eaed] text-[#5f6368] hover:text-[#1a73e8] rounded-xl transition-all"
                title="重新生成"
              >
                <RefreshCw class="w-4 h-4" />
              </button>
              <button
                @click="copyGeneratedPassword"
                class="px-3 py-1.5 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center gap-1.5 shadow-sm transition-all"
                title="复制密码"
              >
                <Copy class="w-3.5 h-3.5" />
                <span>复制</span>
              </button>
            </div>
          </div>

          <!-- Strength Meter -->
          <div class="flex flex-col gap-1 pt-1 border-t border-[#dadce0]/60">
            <div class="flex items-center justify-between text-[11px]">
              <span class="text-[#5f6368]">安全性强度</span>
              <span class="font-bold" :class="passwordStrength.textCol">{{ passwordStrength.text }}</span>
            </div>
            <div class="w-full h-1.5 bg-gray-200 rounded-full overflow-hidden">
              <div
                class="h-full transition-all duration-300"
                :class="passwordStrength.color"
                :style="{ width: passwordStrength.percent }"
              ></div>
            </div>
          </div>
        </div>

        <!-- Presets -->
        <div class="flex items-center gap-2">
          <span class="text-xs text-[#5f6368] font-medium flex-shrink-0">快速预设:</span>
          <div class="flex flex-wrap gap-1.5">
            <button
              @click="applyPwdPreset('strong')"
              class="px-2.5 py-1 bg-[#f1f3f4] hover:bg-[#e8f0fe] hover:text-[#1a73e8] rounded-lg text-xs font-medium transition-colors"
            >
              标准强密 (20位)
            </button>
            <button
              @click="applyPwdPreset('max')"
              class="px-2.5 py-1 bg-[#f1f3f4] hover:bg-[#e8f0fe] hover:text-[#1a73e8] rounded-lg text-xs font-medium transition-colors"
            >
              超高强度 (32位)
            </button>
            <button
              @click="applyPwdPreset('alphanumeric')"
              class="px-2.5 py-1 bg-[#f1f3f4] hover:bg-[#e8f0fe] hover:text-[#1a73e8] rounded-lg text-xs font-medium transition-colors"
            >
              纯字母数字 (18位)
            </button>
            <button
              @click="applyPwdPreset('pin')"
              class="px-2.5 py-1 bg-[#f1f3f4] hover:bg-[#e8f0fe] hover:text-[#1a73e8] rounded-lg text-xs font-medium transition-colors"
            >
              PIN码 (6位)
            </button>
          </div>
        </div>

        <!-- Controls -->
        <div class="space-y-4">
          <!-- Length Slider -->
          <div class="space-y-1.5">
            <div class="flex items-center justify-between text-xs font-medium text-[#202124]">
              <span>密码长度 ({{ pwdGenForm.length }} 位)</span>
              <span class="text-[#5f6368] font-mono">{{ pwdGenForm.length }} / 64</span>
            </div>
            <input
              type="range"
              min="6"
              max="64"
              v-model.number="pwdGenForm.length"
              @input="generateNewPassword"
              class="w-full accent-[#1a73e8] cursor-pointer"
            />
          </div>

          <!-- Character Sets Checkboxes -->
          <div class="grid grid-cols-2 gap-2.5 pt-1">
            <label class="flex items-center gap-2 text-xs text-[#202124] cursor-pointer">
              <input
                type="checkbox"
                v-model="pwdGenForm.uppercase"
                @change="generateNewPassword"
                class="rounded text-[#1a73e8] focus:ring-[#1a73e8]"
              />
              <span>大写字母 (A-Z)</span>
            </label>

            <label class="flex items-center gap-2 text-xs text-[#202124] cursor-pointer">
              <input
                type="checkbox"
                v-model="pwdGenForm.lowercase"
                @change="generateNewPassword"
                class="rounded text-[#1a73e8] focus:ring-[#1a73e8]"
              />
              <span>小写字母 (a-z)</span>
            </label>

            <label class="flex items-center gap-2 text-xs text-[#202124] cursor-pointer">
              <input
                type="checkbox"
                v-model="pwdGenForm.numbers"
                @change="generateNewPassword"
                class="rounded text-[#1a73e8] focus:ring-[#1a73e8]"
              />
              <span>数字 (0-9)</span>
            </label>

            <label class="flex items-center gap-2 text-xs text-[#202124] cursor-pointer">
              <input
                type="checkbox"
                v-model="pwdGenForm.symbols"
                @change="generateNewPassword"
                class="rounded text-[#1a73e8] focus:ring-[#1a73e8]"
              />
              <span>特殊符号 (!@#$...)</span>
            </label>

            <label class="flex items-center gap-2 text-xs text-[#202124] cursor-pointer col-span-2">
              <input
                type="checkbox"
                v-model="pwdGenForm.exclude_ambiguous"
                @change="generateNewPassword"
                class="rounded text-[#1a73e8] focus:ring-[#1a73e8]"
              />
              <span>排除易混淆字符 (避免 1, l, I, 0, O, o)</span>
            </label>
          </div>
        </div>

        <!-- Footer Actions -->
        <div class="flex items-center justify-between pt-3 border-t border-[#dadce0] gap-3">
          <button
            v-if="selectedRecord"
            @click="applyPasswordToCurrentRecord"
            class="px-4 py-2.5 bg-emerald-50 hover:bg-emerald-100 text-emerald-800 border border-emerald-200 rounded-xl text-xs font-semibold flex items-center gap-1.5 transition-all"
          >
            <Sparkles class="w-3.5 h-3.5 text-emerald-600" />
            <span>应用至当前账号 ({{ selectedRecord.email.split('@')[0] }})</span>
          </button>
          <div v-else></div>

          <div class="flex items-center gap-2">
            <button
              @click="showPasswordGenModal = false"
              class="px-4 py-2.5 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium transition-colors"
            >
              关闭
            </button>
            <button
              @click="copyGeneratedPassword"
              class="px-5 py-2.5 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center gap-1.5 shadow-sm transition-all"
            >
              <Copy class="w-3.5 h-3.5" />
              <span>复制密码</span>
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- 9. FLOATING TOAST NOTIFICATION SYSTEM -->
    <div class="fixed top-6 left-1/2 -translate-x-1/2 z-[9999] pointer-events-none flex flex-col gap-2.5 items-center max-w-lg w-full px-4">
      <transition-group
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="transform -translate-y-4 opacity-0 scale-95"
        enter-to-class="transform translate-y-0 opacity-100 scale-100"
        leave-active-class="transition duration-150 ease-in"
        leave-from-class="transform translate-y-0 opacity-100 scale-100"
        leave-to-class="transform -translate-y-2 opacity-0 scale-95"
      >
        <div
          v-for="toast in toasts"
          :key="toast.id"
          class="pointer-events-auto px-4 py-2.5 rounded-2xl shadow-xl border text-xs font-semibold flex items-center gap-2.5 backdrop-blur-xl transition-all select-text"
          :class="{
            'bg-emerald-50/95 text-emerald-900 border-emerald-300/80 shadow-emerald-500/10': toast.type === 'success',
            'bg-red-50/95 text-red-900 border-red-300/80 shadow-red-500/10': toast.type === 'error',
            'bg-amber-50/95 text-amber-900 border-amber-300/80 shadow-amber-500/10': toast.type === 'warning',
            'bg-slate-900/90 text-white border-slate-700/60 shadow-black/25': toast.type === 'info'
          }"
        >
          <CheckCircle2 v-if="toast.type === 'success'" class="w-4 h-4 text-emerald-600 flex-shrink-0" />
          <AlertCircle v-else-if="toast.type === 'error'" class="w-4 h-4 text-red-600 flex-shrink-0" />
          <AlertTriangle v-else-if="toast.type === 'warning'" class="w-4 h-4 text-amber-600 flex-shrink-0" />
          <Info v-else class="w-4 h-4 text-blue-400 flex-shrink-0" />
          <span class="leading-normal">{{ toast.message }}</span>
        </div>
      </transition-group>
    </div>

    <!-- 10. CUSTOM CONFIRMATION MODAL -->
    <div
      v-if="confirmState.visible"
      class="fixed inset-0 z-[9998] flex items-center justify-center p-4 bg-black/45 backdrop-blur-sm"
    >
      <div class="w-full max-w-sm bg-white rounded-3xl p-6 shadow-2xl border border-[#dadce0] flex flex-col gap-4 transform transition-all animate-in fade-in zoom-in-95 duration-150">
        <div class="flex items-start gap-3.5">
          <div
            class="w-10 h-10 rounded-2xl flex items-center justify-center flex-shrink-0"
            :class="{
              'bg-red-100 text-red-600': confirmState.type === 'danger',
              'bg-amber-100 text-amber-700': confirmState.type === 'warning',
              'bg-[#e8f0fe] text-[#1a73e8]': confirmState.type === 'info'
            }"
          >
            <AlertTriangle v-if="confirmState.type === 'warning'" class="w-5 h-5" />
            <Trash2 v-else-if="confirmState.type === 'danger'" class="w-5 h-5" />
            <Info v-else class="w-5 h-5" />
          </div>
          <div class="flex-1">
            <h3 class="text-sm font-bold text-[#202124]">{{ confirmState.title }}</h3>
            <p class="text-xs text-[#5f6368] mt-1.5 leading-relaxed">{{ confirmState.message }}</p>
          </div>
        </div>

        <div class="flex items-center justify-end gap-2.5 mt-2 pt-2 border-t border-[#f1f3f4]">
          <button
            @click="confirmState.resolve(false)"
            class="px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#dadce0] text-[#3c4043] rounded-xl text-xs font-medium transition-colors"
          >
            {{ confirmState.cancelText }}
          </button>
          <button
            @click="confirmState.resolve(true)"
            class="px-4 py-2 text-white rounded-xl text-xs font-medium transition-colors shadow-sm"
            :class="{
              'bg-red-600 hover:bg-red-700 active:bg-red-800': confirmState.type === 'danger',
              'bg-amber-600 hover:bg-amber-700 active:bg-amber-800': confirmState.type === 'warning',
              'bg-[#1a73e8] hover:bg-[#1557b0] active:bg-[#174ea6]': confirmState.type === 'info'
            }"
          >
            {{ confirmState.confirmText }}
          </button>
        </div>
      </div>
    </div>

  </div>
</template>
