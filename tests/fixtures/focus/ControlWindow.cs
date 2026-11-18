using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Threading;

public sealed class RooomFocusControl : IDisposable {
    const uint Quit = 0x0012;
    const uint ActivateWindow = 0x8001;
    const uint GrantForeground = 0x8002;

    [StructLayout(LayoutKind.Sequential)]
    struct Point { public int X, Y; }

    [StructLayout(LayoutKind.Sequential)]
    struct Message {
        public IntPtr Window;
        public uint Kind;
        public UIntPtr WParam;
        public IntPtr LParam;
        public uint Time;
        public Point Position;
        public uint Private;
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateWindowEx(uint extended, string className, string title,
        uint style, int x, int y, int width, int height, IntPtr parent,
        IntPtr menu, IntPtr instance, IntPtr parameter);
    [DllImport("user32.dll")] static extern bool ShowWindow(IntPtr window, int command);
    [DllImport("user32.dll")] static extern bool DestroyWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool AllowSetForegroundWindow(uint process);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] static extern bool AttachThreadInput(uint first, uint second, bool attach);
    [DllImport("user32.dll", SetLastError = true)]
    static extern int GetMessage(out Message message, IntPtr window, uint min, uint max);
    [DllImport("user32.dll")] static extern bool TranslateMessage(ref Message message);
    [DllImport("user32.dll")] static extern IntPtr DispatchMessage(ref Message message);
    [DllImport("user32.dll", SetLastError = true)]
    static extern bool PostThreadMessage(uint thread, uint message, UIntPtr wParam, IntPtr lParam);
    [DllImport("kernel32.dll")] static extern uint GetCurrentThreadId();

    readonly Thread worker;
    readonly ManualResetEventSlim ready = new ManualResetEventSlim();
    readonly ManualResetEventSlim answered = new ManualResetEventSlim();
    uint threadId;
    IntPtr window, previous;
    Exception failure;
    bool result;

    public long Hwnd { get { return window.ToInt64(); } }

    public RooomFocusControl() {
        worker = new Thread(Run) { IsBackground = true, Name = "Rooom focus fixture" };
        worker.Start();
        if (!ready.Wait(5000)) throw new Exception("Focus control did not initialize.");
        if (failure != null) throw new Exception("Focus control failed.", failure);
    }

    void Run() {
        try {
            threadId = GetCurrentThreadId();
            previous = GetForegroundWindow();
            // This fixture owns both its native window and its message loop.
            window = CreateWindowEx(0x80, "STATIC", "Rooom focus regression", 0x00CF0000,
                40, 40, 200, 80, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero);
            if (window == IntPtr.Zero) throw new Win32Exception();
            ShowWindow(window, 4); // SW_SHOWNOACTIVATE
            ready.Set();
            Message message;
            int status;
            while ((status = GetMessage(out message, IntPtr.Zero, 0, 0)) > 0) {
                if (message.Kind == ActivateWindow) {
                    uint process;
                    uint foregroundThread = GetWindowThreadProcessId(GetForegroundWindow(), out process);
                    bool attached = foregroundThread != 0 && foregroundThread != threadId
                        && AttachThreadInput(threadId, foregroundThread, true);
                    try {
                        result = SetForegroundWindow(window);
                    } finally {
                        if (attached) AttachThreadInput(threadId, foregroundThread, false);
                    }
                    answered.Set();
                } else if (message.Kind == GrantForeground) {
                    result = GetForegroundWindow() == window
                        && AllowSetForegroundWindow(message.WParam.ToUInt32());
                    answered.Set();
                } else {
                    TranslateMessage(ref message);
                    DispatchMessage(ref message);
                }
            }
            if (status < 0) throw new Win32Exception();
        } catch (Exception error) {
            failure = error;
            ready.Set();
            answered.Set();
        } finally {
            if (window != IntPtr.Zero) {
                // Respect a window the owner selected after the fixture's activation.
                if (GetForegroundWindow() == window && IsWindow(previous)) SetForegroundWindow(previous);
                DestroyWindow(window);
            }
        }
    }

    bool Request(uint message, uint parameter) {
        answered.Reset();
        if (!PostThreadMessage(threadId, message, new UIntPtr(parameter), IntPtr.Zero))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        if (!answered.Wait(5000)) throw new Exception("Focus control did not answer.");
        if (failure != null) throw new Exception("Focus control failed.", failure);
        return result;
    }

    public bool Activate() { return Request(ActivateWindow, 0); }
    public bool Grant(uint process) { return Request(GrantForeground, process); }

    public void Dispose() {
        if (worker.IsAlive) {
            if (!PostThreadMessage(threadId, Quit, UIntPtr.Zero, IntPtr.Zero))
                throw new Win32Exception(Marshal.GetLastWin32Error());
            if (!worker.Join(5000)) throw new Exception("Focus control did not stop.");
        }
        ready.Dispose();
        answered.Dispose();
    }
}
