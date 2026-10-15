<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  Shield,
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
  User,
  Sparkles,
  Clock,
  Info,
  AlertCircle,
  Compass,
  Upload,
  Cloud,
  History,
  Download,
  KeyRound,
  RefreshCw,
  FileText,
  Fingerprint,
  X
} from "lucide-vue-next";

interface AccountRecord {
  id: string;
  email: string;
  password?: string;
  group?: string;
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

// TOTP Cache & Timer
const totpMap = ref<Record<string, TotpResult>>({});
let totpTimer: number | null = null;

// UI Helpers
const showPassword = ref(false);
const copiedField = ref<string | null>(null);
const isEditing = ref(false);
const editForm = ref<AccountRecord>({
  id: "",
  email: "",
  password: "",
  group: "",
  recovery_email: "",
  phone: "",
  totp: "",
  verification: "",
  backup_codes: "",
  used_backup_codes: "[]",
  notes: "",
  google_one_plan: "",
  google_one_used: "",
  google_one_status: "unknown"
});

// Windows Hello state
const helloStatus = ref<{ available: boolean; enrolled: boolean }>({ available: false, enrolled: false });
const isHelloVerifying = ref(false);

// Check vault on mount
onMounted(async () => {
  try {
    vaultExists.value = await invoke<boolean>("check_vault_exists");
  } catch (e) {
    vaultExists.value = false;
  }
  try {
    helloStatus.value = await invoke<{ available: boolean; enrolled: boolean }>("check_windows_hello_status");
  } catch (e) {
    console.warn("Check Windows Hello error:", e);
  }
});

onUnmounted(() => {
  if (totpTimer) clearInterval(totpTimer);
});

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
    if (records.value.length > 0) {
      selectedId.value = records.value[0].id;
    }
    startTotpTicker();
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
    records.value = await invoke<AccountRecord[]>("unlock_vault_with_hello");
    isUnlocked.value = true;
    if (records.value.length > 0) {
      selectedId.value = records.value[0].id;
    }
    startTotpTicker();
  } catch (err: any) {
    unlockError.value = typeof err === "string" ? err : (err?.message || "Windows Hello 验证未通过");
  } finally {
    isHelloVerifying.value = false;
  }
}

async function toggleWindowsHello() {
  try {
    if (helloStatus.value.enrolled) {
      await invoke("disable_windows_hello");
      helloStatus.value.enrolled = false;
      alert("已停用 Windows Hello / PIN 快速解锁");
    } else {
      await invoke("enable_windows_hello");
      helloStatus.value.enrolled = true;
      alert("✅ Windows Hello / PIN 快速解锁已成功绑定！下次启动可直接使用指纹/面容/PIN一键解锁。");
    }
  } catch (err: any) {
    alert("设置 Windows Hello 失败: " + err);
  }
}

async function lockVault() {
  await invoke("lock_vault");
  isUnlocked.value = false;
  masterPassword.value = "";
  records.value = [];
  selectedId.value = null;
  if (totpTimer) clearInterval(totpTimer);
  try {
    helloStatus.value = await invoke<{ available: boolean; enrolled: boolean }>("check_windows_hello_status");
  } catch (e) {
    // ignore
  }
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
      const match = [r.email, r.group, r.notes, r.recovery_email].join(" ").toLowerCase();
      if (!match.includes(q)) return false;
    }
    return true;
  });
});

// Copy Helper
async function copyText(text: string, fieldKey: string) {
  if (!text) return;
  await navigator.clipboard.writeText(text);
  copiedField.value = fieldKey;
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
    recovery_email: "",
    phone: "",
    totp: "",
    verification: "",
    backup_codes: "",
    used_backup_codes: "[]",
    notes: "",
    google_one_plan: "",
    google_one_used: "",
    google_one_status: "unknown"
  };
  isEditing.value = true;
}

function openEditModal(record: AccountRecord) {
  editForm.value = JSON.parse(JSON.stringify(record));
  isEditing.value = true;
}

async function handleSaveRecord() {
  if (!editForm.value.email.trim()) return;
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
  } catch (err: any) {
    alert("保存失败: " + err);
  }
}

async function handleDeleteRecord(id: string) {
  if (!confirm("确定要从本地安全库中删除该账号记录吗？此操作不可逆。")) return;
  try {
    await invoke("delete_record", { id });
    records.value = records.value.filter((r) => r.id !== id);
    if (selectedId.value === id) {
      selectedId.value = records.value[0]?.id || null;
    }
  } catch (err: any) {
    alert("删除失败: " + err);
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
      url
    });
  } catch (err: any) {
    alert("打开专属独立会话窗口失败: " + err);
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
}

async function copyAndConsumeNextBackupCode() {
  if (!selectedRecord.value) return;
  const all = parsedBackupCodesList.value;
  const used = new Set(usedBackupCodesSet.value);
  const next = all.find(c => !used.has(c));
  if (!next) {
    alert("当前账号所有备用验证码已全部标记为已使用！请在 Google 安全设置中生成新代码。");
    return;
  }
  await navigator.clipboard.writeText(next);
  used.add(next);
  selectedRecord.value.used_backup_codes = JSON.stringify(Array.from(used));
  await invoke("save_record", { record: selectedRecord.value });
  copiedField.value = "backup_code_consumed";
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
}

async function resetAllUsedCodes() {
  if (!selectedRecord.value) return;
  if (!confirm("确定要重置所有备用验证码状态为「未使用」吗？")) return;
  selectedRecord.value.used_backup_codes = "[]";
  await invoke("save_record", { record: selectedRecord.value });
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
    recovery_email: recoveryEmail,
    phone: "",
    totp,
    verification: "",
    backup_codes: backupCodes.join("\n"),
    used_backup_codes: "[]",
    notes: notes.join(" "),
    google_one_plan: "",
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
    alert(`成功批量导入/更新了 ${count} 个账号！`);
    if (!selectedId.value && records.value.length > 0) {
      selectedId.value = records.value[0].id;
    }
    updateAllTotp();
  } catch (err: any) {
    alert("批量导入失败: " + err);
  }
}

// ==================== 3. WEBDAV CLOUD SYNC ====================
const showWebDavModal = ref(false);
const webdavForm = ref({ url: "", username: "", password: "" });
const webdavStatus = ref("");
const isWebDavSyncing = ref(false);

async function openWebDavModal() {
  try {
    const saved = await invoke<any>("load_webdav_config");
    if (saved) {
      webdavForm.value = {
        url: saved.url || "",
        username: saved.username || "",
        password: saved.password || ""
      };
    }
  } catch (e) {
    // ignore
  }
  webdavStatus.value = "";
  showWebDavModal.value = true;
}

async function handleTestWebDav() {
  if (!webdavForm.value.url.trim()) return;
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

async function handleSaveWebDavConfig() {
  try {
    await invoke("save_webdav_config", { config: webdavForm.value });
    webdavStatus.value = "✅ 配置已保存到本地安全存储";
  } catch (err: any) {
    webdavStatus.value = "❌ 保存失败: " + err;
  }
}

async function handleUploadWebDav() {
  isWebDavSyncing.value = true;
  webdavStatus.value = "正在上传加密保险库至云端...";
  try {
    await invoke("save_webdav_config", { config: webdavForm.value });
    await invoke("upload_webdav_vault", { config: webdavForm.value });
    webdavStatus.value = "✅ 本地保险库已成功同步上传至云端（端到端强加密密文）！";
  } catch (err: any) {
    webdavStatus.value = "❌ 上传失败: " + err;
  } finally {
    isWebDavSyncing.value = false;
  }
}

async function handleDownloadWebDav() {
  if (!confirm("从云端恢复将覆盖本地现有库（自动先创建本地快照备份）。确定继续吗？")) return;
  isWebDavSyncing.value = true;
  webdavStatus.value = "正在下载并解密验证云端保险库...";
  try {
    await invoke("save_webdav_config", { config: webdavForm.value });
    const remote = await invoke<AccountRecord[]>("download_webdav_vault", { config: webdavForm.value });
    records.value = remote;
    if (remote.length > 0) selectedId.value = remote[0].id;
    updateAllTotp();
    webdavStatus.value = `✅ 云端恢复成功！共同步载入 ${remote.length} 个账号。`;
  } catch (err: any) {
    webdavStatus.value = "❌ 恢复失败: " + err;
  } finally {
    isWebDavSyncing.value = false;
  }
}

// ==================== 4. BACKUPS & EXPORT ====================
const showBackupsModal = ref(false);
const backupTab = ref<"backups" | "export">("backups");
const backupList = ref<Array<{ filename: string; timestamp: string; size_bytes: number }>>([]);
const isLoadingBackups = ref(false);

async function openBackupsModal() {
  backupTab.value = "backups";
  showBackupsModal.value = true;
  await loadBackupsList();
}

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
  if (!confirm(`确定要回滚到历史版本 [${filename}] 吗？当前数据将先自动创建新的回退快照。`)) return;
  try {
    const restored = await invoke<AccountRecord[]>("restore_vault_backup", { filename });
    records.value = restored;
    if (restored.length > 0) selectedId.value = restored[0].id;
    updateAllTotp();
    await loadBackupsList();
    alert(`成功回滚到历史版本！已载入 ${restored.length} 个账号记录。`);
  } catch (err: any) {
    alert("回滚失败: " + err);
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
  } catch (err: any) {
    alert("导出失败: " + err);
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
  } catch (err: any) {
    alert("导出失败: " + err);
  }
}
</script>

<template>
  <div class="h-screen w-screen flex flex-col bg-[#f8fafd] select-none text-[#1f1f1f] font-sans antialiased overflow-hidden">
    
    <!-- 1. LOCK / UNLOCK SCREEN -->
    <div v-if="!isUnlocked" class="flex-1 flex items-center justify-center p-6 relative">
      <div class="w-full max-w-md bg-white rounded-3xl p-8 shadow-sm border border-[#dadce0]/70 flex flex-col items-center">
        <!-- Google Brand Icon -->
        <div class="w-16 h-16 rounded-2xl bg-[#e8f0fe] flex items-center justify-center mb-5 text-[#1a73e8]">
          <Shield class="w-8 h-8" />
        </div>
        <h1 class="text-2xl font-bold tracking-tight text-[#202124] mb-1">Google 账号管家</h1>
        <p class="text-sm text-[#5f6368] mb-6 flex items-center gap-1.5">
          <span class="w-2 h-2 rounded-full bg-emerald-500"></span>
          Rust 2.0 原生超强加密保险库
        </p>

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

        <!-- Windows Hello Fast Unlock Button -->
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
            <span>{{ isHelloVerifying ? "请在 Windows 弹窗中完成验证..." : "使用 Windows Hello / PIN 快速解锁" }}</span>
          </button>
        </div>

        <div v-else-if="helloStatus.available && vaultExists" class="w-full mt-4 px-3.5 py-2.5 bg-[#f8fafd] border border-[#dadce0] rounded-xl text-xs text-[#5f6368] flex items-center gap-2">
          <Fingerprint class="w-4 h-4 text-[#1a73e8] flex-shrink-0" />
          <span>本机支持 Windows Hello，首次主密码解锁后可在顶部一键绑定指纹/PIN快速登录。</span>
        </div>

        <p class="text-xs text-[#80868b] text-center mt-6">
          所有数据经由 AES-256-GCM 本地加密保存，绝无明文落盘。
        </p>
      </div>
    </div>

    <!-- 2. MAIN APPLICATION WORKSPACE -->
    <template v-else>
      <!-- HEADER TOOLBAR -->
      <header class="h-14 bg-white border-b border-[#dadce0] px-4 flex items-center justify-between gap-4 flex-shrink-0 z-10">
        <!-- Brand -->
        <div class="flex items-center gap-3">
          <div class="w-8 h-8 rounded-lg bg-[#e8f0fe] flex items-center justify-center text-[#1a73e8]">
            <Shield class="w-5 h-5" />
          </div>
          <div class="flex items-baseline gap-2">
            <span class="font-bold text-base text-[#202124] tracking-tight">Google 账号管家</span>
            <span class="text-[11px] font-semibold uppercase px-2 py-0.5 bg-[#e8f0fe] text-[#1a73e8] rounded-full">v3.0.0 Rust</span>
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
            class="absolute right-3 top-1/2 -translate-y-1/2 text-[#5f6368] hover:text-[#202124]"
          >
            <X class="w-3.5 h-3.5" />
          </button>
        </div>

        <!-- Right Toolbar Actions -->
        <div class="flex items-center gap-2">
          <button
            @click="openAddModal"
            class="h-9 px-3.5 bg-[#1a73e8] hover:bg-[#1557b0] active:bg-[#174ea6] text-white rounded-full text-xs font-medium flex items-center gap-1.5 shadow-sm transition-all"
          >
            <Plus class="w-4 h-4" />
            <span>添加账号</span>
          </button>

          <button
            @click="openBatchImportModal"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all"
            title="批量导入多条账号"
          >
            <Upload class="w-3.5 h-3.5 text-[#1a73e8]" />
            <span>批量导入</span>
          </button>

          <button
            @click="openWebDavModal"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all"
            title="WebDAV 云端端到端加密同步"
          >
            <Cloud class="w-3.5 h-3.5 text-[#1a73e8]" />
            <span>云端同步</span>
          </button>

          <button
            @click="openBackupsModal"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all"
            title="历史版本回滚与数据导出"
          >
            <History class="w-3.5 h-3.5 text-[#1a73e8]" />
            <span>备份与导出</span>
          </button>

          <button
            v-if="helloStatus.available"
            @click="toggleWindowsHello"
            class="h-9 px-3 rounded-full text-xs font-medium flex items-center gap-1.5 transition-all"
            :class="helloStatus.enrolled ? 'bg-emerald-50 text-emerald-700 hover:bg-emerald-100 border border-emerald-200' : 'bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043]'"
            :title="helloStatus.enrolled ? '已启用 Windows Hello / PIN 快速解锁 (点击可停用)' : '点击绑定启用 Windows Hello / PIN 快速免密解锁'"
          >
            <Fingerprint class="w-3.5 h-3.5" :class="helloStatus.enrolled ? 'text-emerald-600' : 'text-[#1a73e8]'" />
            <span>{{ helloStatus.enrolled ? "Windows Hello: 已开启" : "开启 Windows Hello" }}</span>
          </button>

          <button
            @click="lockVault"
            class="h-9 px-3 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-full text-xs font-medium flex items-center gap-1.5 transition-all"
            title="锁定保险库"
          >
            <Lock class="w-3.5 h-3.5" />
            <span>锁定</span>
          </button>
        </div>
      </header>

      <!-- THREE-COLUMN MAIN BODY -->
      <div class="flex-1 flex overflow-hidden">
        
        <!-- COLUMN 1: SIDEBAR (GROUPS & STATS) -->
        <aside class="w-56 bg-white border-r border-[#dadce0] flex flex-col flex-shrink-0">
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
        <main class="flex-1 bg-white border-r border-[#dadce0] flex flex-col overflow-hidden">
          <div class="px-4 py-2 bg-[#f8fafd] border-b border-[#dadce0] flex items-center justify-between text-xs text-[#5f6368]">
            <span>共 {{ filteredRecords.length }} 个账号符合筛选</span>
            <span class="text-[11px] text-[#80868b]">双击可快速编辑详情</span>
          </div>

          <div class="flex-1 overflow-y-auto divide-y divide-[#f1f3f4]">
            <div
              v-for="r in filteredRecords"
              :key="r.id"
              @click="selectedId = r.id"
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
                  <div class="text-xs font-semibold text-[#202124] truncate">{{ r.email }}</div>
                  <div class="text-[11px] text-[#5f6368] flex items-center gap-2 mt-0.5">
                    <span v-if="r.group" class="px-1.5 py-0.2 bg-[#f1f3f4] text-[#444746] rounded text-[10px]">{{ r.group }}</span>
                    <span v-if="r.recovery_email" class="truncate">{{ r.recovery_email }}</span>
                  </div>
                </div>
              </div>

              <!-- Right: TOTP & One Badge -->
              <div class="flex items-center gap-4 flex-shrink-0">
                <!-- 2FA Code Badge -->
                <div v-if="r.totp" class="flex flex-col items-end">
                  <div class="flex items-center gap-1.5 font-mono text-sm font-bold text-[#1a73e8]">
                    <span>{{ totpMap[r.id]?.code || "••••••" }}</span>
                    <button
                      @click.stop="copyText(totpMap[r.id]?.code || '', r.id + '-totp')"
                      class="text-[#5f6368] hover:text-[#1a73e8]"
                      title="复制动态验证码"
                    >
                      <Check v-if="copiedField === r.id + '-totp'" class="w-3.5 h-3.5 text-emerald-600" />
                      <Copy v-else class="w-3.5 h-3.5" />
                    </button>
                  </div>
                  <span class="text-[10px] text-[#80868b] font-mono">{{ totpMap[r.id]?.remaining_seconds || 30 }}s 剩余</span>
                </div>
                <div v-else class="text-[11px] text-[#9aa0a6] font-mono">未配 2FA</div>

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
                  class="px-2 py-0.5 rounded-full text-[10px] font-medium bg-gray-100 text-gray-600"
                >
                  非会员
                </div>
                <div
                  v-else
                  class="px-2 py-0.5 rounded-full text-[10px] font-medium bg-[#f1f3f4] text-[#5f6368]"
                >
                  未验证
                </div>
              </div>
            </div>

            <div v-if="filteredRecords.length === 0" class="p-8 text-center text-xs text-[#80868b]">
              暂无匹配的账号记录
            </div>
          </div>
        </main>

        <!-- COLUMN 3: RIGHT DETAIL CARD -->
        <aside class="w-96 bg-[#f8fafd] flex flex-col flex-shrink-0 overflow-y-auto p-4 gap-4">
          <template v-if="selectedRecord">
            <!-- Account Header Card -->
            <div class="bg-white rounded-2xl p-4 border border-[#dadce0]/80 shadow-sm">
              <div class="flex items-center gap-3 mb-3">
                <div class="w-12 h-12 rounded-full bg-[#e8f0fe] text-[#1a73e8] font-bold text-lg flex items-center justify-center flex-shrink-0">
                  {{ (selectedRecord.email[0] || 'G').toUpperCase() }}
                </div>
                <div class="min-w-0 flex-1">
                  <div class="text-xs text-[#5f6368]">Google 账号</div>
                  <div class="text-sm font-bold text-[#202124] truncate">{{ selectedRecord.email }}</div>
                </div>
              </div>

              <!-- Quick Launch Buttons (Shape A: In-App Isolated Windows) -->
              <div class="grid grid-cols-3 gap-2">
                <button
                  @click="openInApp(`https://accounts.google.com/ServiceLogin?Email=${encodeURIComponent(selectedRecord.email)}&login_hint=${encodeURIComponent(selectedRecord.email)}&continue=${encodeURIComponent('https://myaccount.google.com/')}`)"
                  :disabled="isOpeningBrowser"
                  class="h-8 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition-all disabled:opacity-50"
                  title="在专属独立会话窗口中登录此账号"
                >
                  <ExternalLink class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>快捷登录</span>
                </button>
                <button
                  @click="openInApp('https://one.google.com/storage')"
                  :disabled="isOpeningBrowser"
                  class="h-8 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition-all disabled:opacity-50"
                  title="在专属独立会话窗口中打开 Google One"
                >
                  <Globe class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>One 方案</span>
                </button>
                <button
                  @click="openInApp('https://policies.google.com/terms')"
                  :disabled="isOpeningBrowser"
                  class="h-8 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition-all disabled:opacity-50"
                  title="在专属独立会话窗口中查看注册归属地区"
                >
                  <Info class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>归属地区</span>
                </button>
                <button
                  @click="openInApp('https://payments.google.com/gp/w/home/settings')"
                  :disabled="isOpeningBrowser"
                  class="h-8 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition-all col-span-2 disabled:opacity-50"
                  title="在专属独立会话窗口中打开支付与账单"
                >
                  <CreditCard class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>支付设置与账单</span>
                </button>
                <button
                  @click="openCustomUrlModal"
                  class="h-8 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition-all"
                  title="在独立窗口中打开其他服务或自定义网址"
                >
                  <Compass class="w-3.5 h-3.5 text-[#1a73e8]" />
                  <span>更多网址</span>
                </button>
                <button
                  v-if="selectedRecord.recovery_email"
                  @click="copyText(selectedRecord.recovery_email, 'rec_email')"
                  class="h-8 px-2 bg-[#f1f3f4] hover:bg-[#e8eaed] active:bg-[#e2e5e9] text-[#3c4043] rounded-lg text-xs font-medium flex items-center justify-center gap-1 transition-all col-span-3 truncate"
                >
                  <Copy class="w-3.5 h-3.5 flex-shrink-0" />
                  <span class="truncate">{{ copiedField === 'rec_email' ? '已复制辅助邮箱' : '复制辅助邮箱：' + selectedRecord.recovery_email }}</span>
                </button>
              </div>
            </div>

            <!-- Credentials & Structured Info -->
            <div class="bg-white rounded-2xl p-4 border border-[#dadce0]/80 shadow-sm space-y-3">
              <!-- Password Row -->
              <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4]">
                <div>
                  <div class="text-[11px] text-[#5f6368]">登录密码</div>
                  <div class="font-mono text-sm font-semibold text-[#202124] tracking-wide mt-0.5">
                    {{ showPassword ? (selectedRecord.password || "未记录") : "••••••••••••" }}
                  </div>
                </div>
                <div class="flex items-center gap-1">
                  <button
                    @click="showPassword = !showPassword"
                    class="p-1.5 hover:bg-[#f1f3f4] rounded-lg text-[#5f6368] transition-colors"
                    title="显示 / 隐藏"
                  >
                    <EyeOff v-if="showPassword" class="w-4 h-4" />
                    <Eye v-else class="w-4 h-4" />
                  </button>
                  <button
                    @click="copyText(selectedRecord.password || '', 'pwd')"
                    class="p-1.5 hover:bg-[#f1f3f4] rounded-lg text-[#5f6368] transition-colors"
                    title="复制密码"
                  >
                    <Check v-if="copiedField === 'pwd'" class="w-4 h-4 text-emerald-600" />
                    <Copy v-else class="w-4 h-4" />
                  </button>
                </div>
              </div>

              <!-- Information Grid -->
              <div class="grid grid-cols-2 gap-y-2 text-xs">
                <div class="text-[#5f6368]">所属分组</div>
                <div class="text-[#202124] font-medium">{{ selectedRecord.group || "未分组" }}</div>

                <div class="text-[#5f6368]">恢复邮箱</div>
                <div class="text-[#1a73e8] font-medium truncate">{{ selectedRecord.recovery_email || "—" }}</div>

                <div class="text-[#5f6368]">恢复手机</div>
                <div class="text-[#202124] font-medium">{{ selectedRecord.phone || "—" }}</div>

                <div class="text-[#5f6368]">验证方式</div>
                <div class="text-[#202124] font-medium">{{ selectedRecord.verification || "—" }}</div>

                <div class="text-[#5f6368]">Google One</div>
                <div class="text-[#202124] font-medium">
                  {{ selectedRecord.google_one_plan ? (selectedRecord.google_one_plan + (selectedRecord.google_one_used ? ' (' + selectedRecord.google_one_used + ')' : '')) : '—' }}
                </div>
              </div>

              <!-- Notes -->
              <div v-if="selectedRecord.notes" class="pt-2 border-t border-[#f1f3f4] text-xs text-[#5f6368]">
                <div class="font-medium mb-1">备注信息：</div>
                <div class="p-2 bg-[#f8fafd] rounded-xl text-[#202124] leading-relaxed">{{ selectedRecord.notes }}</div>
              </div>
            </div>

            <!-- TOTP Card -->
            <div class="bg-[#edf2fa] rounded-2xl p-4 border border-[#d3e3fd]/80">
              <div class="flex items-center justify-between mb-2">
                <span class="text-xs font-bold text-[#444746] flex items-center gap-1.5">
                  <Clock class="w-4 h-4 text-[#1a73e8]" />
                  2FA 动态安全码 (TOTP)
                </span>
                <span class="text-xs font-mono text-[#5f6368]">{{ totpMap[selectedRecord.id]?.remaining_seconds || 30 }}s 后刷新</span>
              </div>

              <div class="flex items-center justify-between my-2">
                <div class="font-mono text-3xl font-extrabold tracking-widest text-[#1a73e8]">
                  {{ totpMap[selectedRecord.id]?.code || "未配置" }}
                </div>
                <button
                  v-if="selectedRecord.totp"
                  @click="copyText(totpMap[selectedRecord.id]?.code || '', 'totp_large')"
                  class="px-3 py-1.5 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center gap-1.5 shadow-sm transition-all"
                >
                  <Check v-if="copiedField === 'totp_large'" class="w-3.5 h-3.5" />
                  <Copy v-else class="w-3.5 h-3.5" />
                  <span>{{ copiedField === 'totp_large' ? '已复制' : '复制' }}</span>
                </button>
              </div>
            </div>

            <!-- BACKUP CODES CARD -->
            <div class="bg-white rounded-2xl p-4 border border-[#dadce0]/80 shadow-sm space-y-3">
              <div class="flex items-center justify-between">
                <span class="text-xs font-bold text-[#202124] flex items-center gap-1.5">
                  <KeyRound class="w-4 h-4 text-[#1a73e8]" />
                  Google 备用验证码
                </span>
                <span
                  v-if="parsedBackupCodesList.length > 0"
                  :class="[
                    'text-[11px] font-semibold px-2 py-0.5 rounded-full font-mono',
                    unusedBackupCodesCount > 0 ? 'bg-emerald-50 text-emerald-700 border border-emerald-200' : 'bg-rose-50 text-rose-700 border border-rose-200'
                  ]"
                >
                  剩余 {{ unusedBackupCodesCount }} / {{ parsedBackupCodesList.length }}
                </span>
              </div>

              <div v-if="parsedBackupCodesList.length > 0" class="space-y-2">
                <button
                  @click="copyAndConsumeNextBackupCode"
                  :disabled="unusedBackupCodesCount === 0"
                  class="w-full py-2 bg-[#1a73e8] hover:bg-[#1557b0] active:bg-[#174ea6] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-sm transition-all disabled:opacity-50"
                  title="自动复制下一个未使用的备用码，并立即标记为已使用"
                >
                  <Check v-if="copiedField === 'backup_code_consumed'" class="w-3.5 h-3.5" />
                  <Copy v-else class="w-3.5 h-3.5" />
                  <span>{{ copiedField === 'backup_code_consumed' ? '已复制并自动核销！' : '⚡ 复制下一个可用码并核销' }}</span>
                </button>

                <button
                  @click="openBackupCodesModal"
                  class="w-full py-1.5 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium flex items-center justify-center gap-1 transition-all"
                >
                  <span>查看全部备用码与核销状态</span>
                </button>
              </div>

              <div v-else class="text-center py-2">
                <p class="text-xs text-[#80868b] mb-2">尚未配置备用验证码</p>
                <button
                  @click="openBackupCodesModal"
                  class="px-3 py-1 bg-[#e8f0fe] hover:bg-[#d2e3fc] text-[#1a73e8] rounded-lg text-xs font-medium transition-all"
                >
                  + 批量粘贴备用码
                </button>
              </div>
            </div>

            <!-- Action Buttons -->
            <div class="space-y-2 pt-2">
              <button
                @click="openEditModal(selectedRecord)"
                class="w-full py-2.5 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-2 shadow-sm transition-all"
              >
                <Edit3 class="w-4 h-4" />
                <span>编辑该账号资料</span>
              </button>
              <button
                @click="handleDeleteRecord(selectedRecord.id)"
                class="w-full py-2 text-rose-600 hover:bg-rose-50 rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-colors"
              >
                <Trash2 class="w-3.5 h-3.5" />
                <span>删除此本地记录</span>
              </button>
            </div>
          </template>

          <div v-else class="flex-1 flex flex-col items-center justify-center text-center p-6 text-[#80868b]">
            <User class="w-12 h-12 mb-3 text-[#dadce0]" />
            <p class="text-xs">请在左侧或中间列表选择一个账号查看详情</p>
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

          <div class="grid grid-cols-2 gap-3">
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">Google One 方案</label>
              <input
                type="text"
                v-model="editForm.google_one_plan"
                placeholder="100 GB / 2 TB..."
                class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
              />
            </div>
            <div>
              <label class="block text-xs font-semibold text-[#5f6368] mb-1">已用容量</label>
              <input
                type="text"
                v-model="editForm.google_one_used"
                placeholder="15 GB / 350 GB..."
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
              class="px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#444746] rounded-xl text-xs font-medium"
            >
              取消
            </button>
            <button
              type="submit"
              class="px-5 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium shadow-sm"
            >
              保存至安全库
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

    <!-- 7. WEBDAV CLOUD SYNC MODAL -->
    <div
      v-if="showWebDavModal"
      class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="w-full max-w-md bg-white rounded-3xl p-6 shadow-xl border border-[#dadce0] flex flex-col">
        <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4] mb-4">
          <div class="flex items-center gap-2">
            <div class="w-7 h-7 rounded-lg bg-[#e8f0fe] flex items-center justify-center text-[#1a73e8]">
              <Cloud class="w-4 h-4" />
            </div>
            <div>
              <h2 class="text-base font-bold text-[#202124]">WebDAV 端到端加密同步</h2>
              <p class="text-[11px] text-[#5f6368]">支持坚果云、Nextcloud 等标准 WebDAV 网盘</p>
            </div>
          </div>
          <button @click="showWebDavModal = false" class="p-1.5 hover:bg-[#f1f3f4] rounded-full text-[#5f6368]">
            <X class="w-4 h-4" />
          </button>
        </div>

        <div class="space-y-3">
          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">WebDAV 服务器地址</label>
            <input
              type="text"
              v-model="webdavForm.url"
              placeholder="https://dav.jianguoyun.com/dav/GoogleVault/"
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-mono"
            />
          </div>

          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">账号 / 邮箱</label>
            <input
              type="text"
              v-model="webdavForm.username"
              placeholder="your_email@example.com"
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none"
            />
          </div>

          <div>
            <label class="block text-xs font-semibold text-[#5f6368] mb-1">应用密码 / Token</label>
            <input
              type="password"
              v-model="webdavForm.password"
              placeholder="网盘生成的专用应用授权密码..."
              class="w-full px-3 py-2 bg-[#f1f3f4] focus:bg-white rounded-xl border border-transparent focus:border-[#1a73e8] text-xs outline-none font-mono"
            />
          </div>

          <div v-if="webdavStatus" class="p-2.5 bg-[#f8fafd] rounded-xl border border-[#dadce0] text-xs leading-relaxed text-[#3c4043]">
            {{ webdavStatus }}
          </div>

          <!-- Buttons -->
          <div class="grid grid-cols-2 gap-2 pt-2">
            <button
              type="button"
              @click="handleTestWebDav"
              :disabled="isWebDavSyncing"
              class="py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 transition-colors disabled:opacity-50"
            >
              <RefreshCw :class="['w-3.5 h-3.5 text-[#1a73e8]', isWebDavSyncing ? 'animate-spin' : '']" />
              <span>测试连接</span>
            </button>
            <button
              type="button"
              @click="handleSaveWebDavConfig"
              class="py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium transition-colors"
            >
              <span>保存配置</span>
            </button>
          </div>

          <div class="grid grid-cols-2 gap-2 pt-1 border-t border-[#f1f3f4]">
            <button
              type="button"
              @click="handleUploadWebDav"
              :disabled="isWebDavSyncing"
              class="py-2.5 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-sm transition-all disabled:opacity-50"
            >
              <Upload class="w-3.5 h-3.5" />
              <span>⬆ 上传备份至云端</span>
            </button>
            <button
              type="button"
              @click="handleDownloadWebDav"
              :disabled="isWebDavSyncing"
              class="py-2.5 bg-emerald-600 hover:bg-emerald-700 text-white rounded-xl text-xs font-medium flex items-center justify-center gap-1.5 shadow-sm transition-all disabled:opacity-50"
            >
              <Download class="w-3.5 h-3.5" />
              <span>⬇ 从云端恢复覆盖</span>
            </button>
          </div>

          <p class="text-[11px] text-[#80868b] text-center pt-1">
            传输与云端存储均为 AES-256-GCM 强加密密文，无主密码即使云服务商也无法解密。
          </p>
        </div>
      </div>
    </div>

    <!-- 8. BACKUPS & EXPORT MODAL -->
    <div
      v-if="showBackupsModal"
      class="fixed inset-0 bg-black/40 backdrop-blur-sm z-50 flex items-center justify-center p-4"
    >
      <div class="w-full max-w-lg bg-white rounded-3xl p-6 shadow-xl border border-[#dadce0] flex flex-col max-h-[85vh]">
        <div class="flex items-center justify-between pb-3 border-b border-[#f1f3f4] mb-3">
          <div class="flex items-center gap-2">
            <div class="w-7 h-7 rounded-lg bg-[#e8f0fe] flex items-center justify-center text-[#1a73e8]">
              <History class="w-4 h-4" />
            </div>
            <div>
              <h2 class="text-base font-bold text-[#202124]">历史版本快照与数据导出</h2>
              <p class="text-[11px] text-[#5f6368]">自动轮转备份防丢失，支持任意历史节点回滚</p>
            </div>
          </div>
          <button @click="showBackupsModal = false" class="p-1.5 hover:bg-[#f1f3f4] rounded-full text-[#5f6368]">
            <X class="w-4 h-4" />
          </button>
        </div>

        <!-- Tabs -->
        <div class="flex gap-2 p-1 bg-[#f1f3f4] rounded-xl mb-4 text-xs font-medium">
          <button
            @click="backupTab = 'backups'"
            :class="[
              'flex-1 py-1.5 rounded-lg transition-all',
              backupTab === 'backups' ? 'bg-white text-[#1a73e8] shadow-sm font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
            ]"
          >
            历史快照回滚 ({{ backupList.length }})
          </button>
          <button
            @click="backupTab = 'export'"
            :class="[
              'flex-1 py-1.5 rounded-lg transition-all',
              backupTab === 'export' ? 'bg-white text-[#1a73e8] shadow-sm font-semibold' : 'text-[#5f6368] hover:text-[#202124]'
            ]"
          >
            明文 / 密文数据导出
          </button>
        </div>

        <!-- Tab 1: Backups -->
        <div v-if="backupTab === 'backups'" class="flex-1 overflow-y-auto space-y-2 pr-1">
          <div v-if="isLoadingBackups" class="py-8 text-center text-xs text-[#80868b]">
            正在扫描本地备份目录...
          </div>
          <div v-else-if="backupList.length === 0" class="py-8 text-center text-xs text-[#80868b]">
            暂无历史快照备份（每次修改数据时会自动生成快照）
          </div>
          <div
            v-else
            v-for="b in backupList"
            :key="b.filename"
            class="p-3 bg-[#f8fafd] hover:bg-white border border-[#dadce0]/70 rounded-xl flex items-center justify-between transition-colors"
          >
            <div>
              <div class="font-mono text-xs font-semibold text-[#202124]">{{ b.timestamp }}</div>
              <div class="text-[11px] text-[#80868b] mt-0.5 font-mono truncate max-w-[260px]">
                {{ b.filename }} ({{ (b.size_bytes / 1024).toFixed(1) }} KB)
              </div>
            </div>
            <button
              @click="handleRestoreBackup(b.filename)"
              class="px-3 py-1.5 bg-[#e8f0fe] hover:bg-[#d2e3fc] text-[#1a73e8] rounded-lg text-xs font-medium transition-colors"
            >
              回滚此版本
            </button>
          </div>
        </div>

        <!-- Tab 2: Export -->
        <div v-else class="space-y-4 py-2">
          <div class="p-3 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-1">
            <h3 class="text-xs font-bold text-[#202124]">导出为标准 CSV 表格</h3>
            <p class="text-[11px] text-[#5f6368] leading-relaxed">
              包含邮箱、登录密码、分组、辅助邮箱、TOTP密钥、备用验证码等全部字段，支持 Excel 直接打开。
            </p>
            <button
              @click="handleExportCsv"
              class="mt-2 px-4 py-2 bg-[#1a73e8] hover:bg-[#1557b0] text-white rounded-xl text-xs font-medium flex items-center gap-1.5 shadow-sm transition-all"
            >
              <FileText class="w-3.5 h-3.5" />
              <span>导出 CSV 表格文件</span>
            </button>
          </div>

          <div class="p-3 bg-[#f8fafd] rounded-2xl border border-[#dadce0] space-y-1">
            <h3 class="text-xs font-bold text-[#202124]">导出为 JSON 数据格式</h3>
            <p class="text-[11px] text-[#5f6368] leading-relaxed">
              适合迁移至其他数据库或自行解析备份的结构化数据。
            </p>
            <button
              @click="handleExportJson"
              class="mt-2 px-4 py-2 bg-[#f1f3f4] hover:bg-[#e8eaed] text-[#3c4043] rounded-xl text-xs font-medium flex items-center gap-1.5 transition-colors"
            >
              <Download class="w-3.5 h-3.5 text-[#1a73e8]" />
              <span>导出 JSON 数据文件</span>
            </button>
          </div>
        </div>
      </div>
    </div>

  </div>
</template>