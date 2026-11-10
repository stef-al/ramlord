// Probe: which of the Win32 calls RAMlord needs behave under Wine?
#define UNICODE
#define _UNICODE
#define _WIN32_WINNT 0x0A00
#include <windows.h>
#include <tlhelp32.h>
#include <psapi.h>
#include <stdio.h>
#include <stdlib.h>

typedef struct {
  DWORD cb; DWORD PageFaultCount; SIZE_T PeakWorkingSetSize; SIZE_T WorkingSetSize;
  SIZE_T QuotaPeakPagedPoolUsage; SIZE_T QuotaPagedPoolUsage; SIZE_T QuotaPeakNonPagedPoolUsage;
  SIZE_T QuotaNonPagedPoolUsage; SIZE_T PagefileUsage; SIZE_T PeakPagefileUsage; SIZE_T PrivateUsage;
  SIZE_T PrivateWorkingSetSize; ULONG64 SharedCommitUsage;
} PMC_EX2;

static FILE *rep;
static unsigned char frame[384 * 240 * 4];
static int visible_windows = 0;

static BOOL CALLBACK enum_cb(HWND h, LPARAM lp) {
  if (IsWindowVisible(h)) {
    DWORD pid = 0; GetWindowThreadProcessId(h, &pid);
    WCHAR cls[64] = L""; GetClassNameW(h, cls, 64);
    RECT r; GetWindowRect(h, &r);
    fprintf(rep, "  visible window pid=%lu class=%ls %ldx%ld\n", pid, cls, r.right - r.left, r.bottom - r.top);
    visible_windows++;
  }
  return TRUE;
}

static BOOL CALLBACK close_cb(HWND h, LPARAM lp) {
  DWORD pid = 0; GetWindowThreadProcessId(h, &pid);
  if (pid == (DWORD)lp && IsWindowVisible(h)) PostMessageW(h, WM_CLOSE, 0, 0);
  return TRUE;
}

static LRESULT CALLBACK wndproc(HWND h, UINT m, WPARAM w, LPARAM l) {
  switch (m) {
    case WM_PAINT: {
      PAINTSTRUCT ps; HDC dc = BeginPaint(h, &ps);
      RECT rc; GetClientRect(h, &rc);
      BITMAPINFO bi; ZeroMemory(&bi, sizeof bi);
      bi.bmiHeader.biSize = sizeof(BITMAPINFOHEADER); bi.bmiHeader.biWidth = 384; bi.bmiHeader.biHeight = -240;
      bi.bmiHeader.biPlanes = 1; bi.bmiHeader.biBitCount = 32; bi.bmiHeader.biCompression = BI_RGB;
      SetStretchBltMode(dc, COLORONCOLOR);
      StretchDIBits(dc, 0, 0, rc.right, rc.bottom, 0, 0, 384, 240, frame, &bi, DIB_RGB_COLORS, SRCCOPY);
      EndPaint(h, &ps); return 0; }
    case WM_TIMER: DestroyWindow(h); return 0;
    case WM_DESTROY: PostQuitMessage(0); return 0;
  }
  return DefWindowProcW(h, m, w, l);
}

static void probe_process(DWORD pid, DWORD ppid, const WCHAR *exe) {
  HANDLE h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
  if (!h) { fprintf(rep, "  pid=%lu ppid=%lu %ls: OpenProcess failed %lu\n", pid, ppid, exe, GetLastError()); return; }
  WCHAR path[MAX_PATH] = L"?"; DWORD n = MAX_PATH; BOOL okp = QueryFullProcessImageNameW(h, 0, path, &n);
  FILETIME c, e, k, u; BOOL okt = GetProcessTimes(h, &c, &e, &k, &u);
  DWORD sess = 9999; BOOL oks = ProcessIdToSessionId(pid, &sess);
  PMC_EX2 m; ZeroMemory(&m, sizeof m); m.cb = sizeof m;
  BOOL ok2 = GetProcessMemoryInfo(h, (PROCESS_MEMORY_COUNTERS *)&m, sizeof m);
  DWORD err2 = ok2 ? 0 : GetLastError();
  PROCESS_MEMORY_COUNTERS_EX mx; ZeroMemory(&mx, sizeof mx); mx.cb = sizeof mx;
  BOOL okx = GetProcessMemoryInfo(h, (PROCESS_MEMORY_COUNTERS *)&mx, sizeof mx);
  fprintf(rep, "  pid=%lu ppid=%lu %ls path_ok=%d times_ok=%d session_ok=%d(%lu) EX2_ok=%d(err %lu) privWS=%llu KB | EX_ok=%d WS=%llu KB commit=%llu KB\n",
          pid, ppid, exe, okp, okt, oks, sess, ok2, err2, (unsigned long long)m.PrivateWorkingSetSize / 1024,
          okx, (unsigned long long)mx.WorkingSetSize / 1024, (unsigned long long)mx.PagefileUsage / 1024);
  if (okp) fprintf(rep, "      image=%ls\n", path);
  CloseHandle(h);
}

int WINAPI wWinMain(HINSTANCE hi, HINSTANCE hp, PWSTR cmd, int show) {
  int secs = _wtoi(cmd); if (secs <= 0) secs = 5;
  rep = fopen("report.txt", "w");
  fprintf(rep, "== system memory\n");
  PERFORMANCE_INFORMATION pi; ZeroMemory(&pi, sizeof pi); pi.cb = sizeof pi;
  BOOL okpi = GetPerformanceInfo(&pi, sizeof pi);
  fprintf(rep, "GetPerformanceInfo ok=%d total=%llu MB avail=%llu MB commit=%llu/%llu MB page=%llu\n", okpi,
          (unsigned long long)pi.PhysicalTotal * pi.PageSize >> 20, (unsigned long long)pi.PhysicalAvailable * pi.PageSize >> 20,
          (unsigned long long)pi.CommitTotal * pi.PageSize >> 20, (unsigned long long)pi.CommitLimit * pi.PageSize >> 20,
          (unsigned long long)pi.PageSize);
  ULONGLONG inst = 0; BOOL oki = GetPhysicallyInstalledSystemMemory(&inst);
  fprintf(rep, "GetPhysicallyInstalledSystemMemory ok=%d (err %lu) %llu MB\n", oki, oki ? 0 : GetLastError(), inst >> 10);
  UINT need = GetSystemFirmwareTable('RSMB', 0, NULL, 0);
  fprintf(rep, "GetSystemFirmwareTable RSMB size=%u (err %lu)\n", need, need ? 0 : GetLastError());

  fprintf(rep, "== start notepad twice (one to close politely, one to force)\n");
  STARTUPINFOW si; PROCESS_INFORMATION p1, p2; WCHAR c1[] = L"notepad.exe", c2[] = L"notepad.exe";
  ZeroMemory(&si, sizeof si); si.cb = sizeof si;
  BOOL s1 = CreateProcessW(NULL, c1, NULL, NULL, FALSE, 0, NULL, NULL, &si, &p1);
  BOOL s2 = CreateProcessW(NULL, c2, NULL, NULL, FALSE, 0, NULL, NULL, &si, &p2);
  fprintf(rep, "notepad started: %d %d\n", s1, s2);
  if (s1) WaitForInputIdle(p1.hProcess, 8000);
  if (s2) WaitForInputIdle(p2.hProcess, 8000);
  Sleep(1500);

  fprintf(rep, "== processes\n");
  HANDLE snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
  PROCESSENTRY32W pe; pe.dwSize = sizeof pe; int count = 0;
  if (snap != INVALID_HANDLE_VALUE && Process32FirstW(snap, &pe)) {
    do { probe_process(pe.th32ProcessID, pe.th32ParentProcessID, pe.szExeFile); count++; } while (Process32NextW(snap, &pe));
  }
  fprintf(rep, "process count=%d self=%lu\n", count, GetCurrentProcessId());

  fprintf(rep, "== windows\n");
  EnumWindows(enum_cb, 0);
  DWORD fpid = 0; HWND fg = GetForegroundWindow(); if (fg) GetWindowThreadProcessId(fg, &fpid);
  fprintf(rep, "visible windows=%d foreground pid=%lu\n", visible_windows, fpid);

  fprintf(rep, "== evict\n");
  if (s1) { EnumWindows(close_cb, (LPARAM)p1.dwProcessId);
    DWORD w = WaitForSingleObject(p1.hProcess, 5000);
    fprintf(rep, "polite WM_CLOSE: %s\n", w == WAIT_OBJECT_0 ? "process exited" : "still running after 5 s"); }
  if (s2) { HANDLE t = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, FALSE, p2.dwProcessId);
    BOOL okt = t && TerminateProcess(t, 1); DWORD w = t ? WaitForSingleObject(t, 5000) : WAIT_FAILED;
    fprintf(rep, "force TerminateProcess ok=%d: %s\n", okt, w == WAIT_OBJECT_0 ? "process exited" : "still running"); }

  fprintf(rep, "== window\n");
  FILE *f = fopen("frame.bin", "rb"); size_t got = f ? fread(frame, 1, sizeof frame, f) : 0; if (f) fclose(f);
  fprintf(rep, "frame bytes=%u\n", (unsigned)got);
  BOOL dpi = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
  fprintf(rep, "SetProcessDpiAwarenessContext ok=%d\n", dpi);
  WNDCLASSW wc; ZeroMemory(&wc, sizeof wc); wc.lpfnWndProc = wndproc; wc.hInstance = hi; wc.lpszClassName = L"RamlordProbe";
  wc.hCursor = LoadCursorW(NULL, IDC_ARROW); RegisterClassW(&wc);
  DWORD style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
  RECT r = {0, 0, 1152, 720}; AdjustWindowRect(&r, style, FALSE);
  HWND h = CreateWindowExW(0, L"RamlordProbe", L"RAMlord", style, 40, 40, r.right - r.left, r.bottom - r.top, NULL, NULL, hi, NULL);
  fprintf(rep, "CreateWindowExW ok=%d\n", h != NULL);
  HANDLE mtx = CreateMutexW(NULL, TRUE, L"Local\\RamlordProbeSingleInstance");
  fprintf(rep, "named mutex ok=%d already=%d\n", mtx != NULL, GetLastError() == ERROR_ALREADY_EXISTS);
  if (OpenClipboard(h)) { EmptyClipboard();
    HGLOBAL g = GlobalAlloc(GMEM_MOVEABLE, sizeof(BITMAPINFOHEADER) + 16);
    BITMAPINFOHEADER *bh = (BITMAPINFOHEADER *)GlobalLock(g); ZeroMemory(bh, sizeof *bh + 16);
    bh->biSize = sizeof *bh; bh->biWidth = 2; bh->biHeight = 2; bh->biPlanes = 1; bh->biBitCount = 32; GlobalUnlock(g);
    HANDLE sc = SetClipboardData(CF_DIB, g); CloseClipboard();
    fprintf(rep, "clipboard CF_DIB ok=%d\n", sc != NULL); } else fprintf(rep, "OpenClipboard failed\n");
  fflush(rep);
  ShowWindow(h, SW_SHOW); UpdateWindow(h); SetTimer(h, 1, secs * 1000, NULL);
  MSG msg; int paints = 0; while (GetMessageW(&msg, NULL, 0, 0) > 0) { if (msg.message == WM_PAINT) paints++; TranslateMessage(&msg); DispatchMessageW(&msg); }
  fprintf(rep, "message loop ended, WM_PAINT seen=%d\n", paints);
  fclose(rep); return 0;
}
