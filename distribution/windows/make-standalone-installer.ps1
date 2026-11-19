# ==============================================================================
# FantasTech — Standalone Native Windows Installer Generator
# ==============================================================================
# Generates a self-contained, standalone Windows setup executable
# (dist\FantasTech-Setup-0.1.0.exe) using Windows built-in .NET Framework
# compiler. Requires ZERO external tools or downloads.

param(
    [string]$OutputDir = "$PSScriptRoot\..\..\dist",
    [string]$ExePath = "$PSScriptRoot\..\..\core\target\release\fantastech.exe",
    [string]$IconPath = "$PSScriptRoot\..\..\assets\icons\fantastech.ico",
    [string]$Version = "0.1.5"
)

$ErrorActionPreference = "Stop"

if (-not (Test-Path $ExePath)) {
    Write-Error "Payload binary not found: $ExePath. Run 'cargo build --release' first."
    exit 1
}

if (-not (Test-Path $OutputDir)) {
    New-Item -ItemType Directory -Path $OutputDir -Force | Out-Null
}

$OutputSetupExe = Join-Path $OutputDir "FantasTech-Setup-$Version.exe"

Write-Host "Compiling standalone Windows installer using native .NET compiler..." -ForegroundColor Cyan

# Read binary bytes and encode to compressed base64
$exeBytes = [System.IO.File]::ReadAllBytes((Resolve-Path $ExePath))
$ms = New-Object System.IO.MemoryStream
$gzip = New-Object System.IO.Compression.GZipStream($ms, [System.IO.Compression.CompressionMode]::Compress)
$gzip.Write($exeBytes, 0, $exeBytes.Length)
$gzip.Close()
$compressedExeBase64 = [Convert]::ToBase64String($ms.ToArray())

$iconBytes = if (Test-Path $IconPath) { [System.IO.File]::ReadAllBytes((Resolve-Path $IconPath)) } else { @() }
$iconBase64 = if ($iconBytes.Length -gt 0) { [Convert]::ToBase64String($iconBytes) } else { "" }

# C# Source for the Standalone Setup GUI Application
$csharpSource = @"
using System;
using System.IO;
using System.IO.Compression;
using System.Diagnostics;
using System.Drawing;
using System.Windows.Forms;
using Microsoft.Win32;
using System.Runtime.InteropServices;

namespace FantasTechInstaller
{
    public class SetupProgram
    {
        private const string AppName = "FantasTech";
        private const string AppVersion = "$Version";
        private const string CompressedExeData = "$compressedExeBase64";
        private const string IconData = "$iconBase64";

        [STAThread]
        public static void Main(string[] args)
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);

            // Check if silent flag
            bool silent = false;
            foreach (var arg in args)
            {
                if (arg.Equals("/S", StringComparison.OrdinalIgnoreCase) ||
                    arg.Equals("/SILENT", StringComparison.OrdinalIgnoreCase) ||
                    arg.Equals("/VERYSILENT", StringComparison.OrdinalIgnoreCase))
                {
                    silent = true;
                }
            }

            if (silent)
            {
                PerformInstall();
                return;
            }

            // Show Graphical Setup Wizard Form
            Form form = new Form();
            form.Text = "FantasTech Setup - Version " + AppVersion;
            form.Size = new Size(520, 360);
            form.StartPosition = FormStartPosition.CenterScreen;
            form.FormBorderStyle = FormBorderStyle.FixedDialog;
            form.MaximizeBox = false;
            form.MinimizeBox = false;
            form.BackColor = Color.FromArgb(245, 247, 250);

            Label titleLabel = new Label();
            titleLabel.Text = "Welcome to FantasTech Setup";
            titleLabel.Font = new Font("Segoe UI", 14, FontStyle.Bold);
            titleLabel.ForeColor = Color.FromArgb(24, 30, 42);
            titleLabel.Location = new Point(24, 20);
            titleLabel.AutoSize = true;
            form.Controls.Add(titleLabel);

            Label descLabel = new Label();
            descLabel.Text = "This wizard will install FantasTech — Fantech MK894 RGB Keyboard Control Suite\non your computer.\n\nAll active instances of FantasTech will be closed automatically during setup.";
            descLabel.Font = new Font("Segoe UI", 9.5f);
            descLabel.ForeColor = Color.FromArgb(80, 90, 105);
            descLabel.Location = new Point(26, 65);
            descLabel.Size = new Size(460, 65);
            form.Controls.Add(descLabel);

            CheckBox chkDesktop = new CheckBox();
            chkDesktop.Text = "Create a Desktop shortcut";
            chkDesktop.Font = new Font("Segoe UI", 9.5f);
            chkDesktop.Checked = true;
            chkDesktop.Location = new Point(28, 145);
            chkDesktop.AutoSize = true;
            form.Controls.Add(chkDesktop);

            CheckBox chkAutostart = new CheckBox();
            chkAutostart.Text = "Start FantasTech background service automatically on user login";
            chkAutostart.Font = new Font("Segoe UI", 9.5f);
            chkAutostart.Checked = true;
            chkAutostart.Location = new Point(28, 175);
            chkAutostart.AutoSize = true;
            form.Controls.Add(chkAutostart);

            CheckBox chkLaunch = new CheckBox();
            chkLaunch.Text = "Launch FantasTech now";
            chkLaunch.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
            chkLaunch.Checked = true;
            chkLaunch.Location = new Point(28, 205);
            chkLaunch.AutoSize = true;
            form.Controls.Add(chkLaunch);

            Panel bottomPanel = new Panel();
            bottomPanel.Dock = DockStyle.Bottom;
            bottomPanel.Height = 55;
            bottomPanel.BackColor = Color.FromArgb(235, 238, 242);
            form.Controls.Add(bottomPanel);

            Button btnCancel = new Button();
            btnCancel.Text = "Cancel";
            btnCancel.Font = new Font("Segoe UI", 9f);
            btnCancel.Size = new Size(90, 32);
            btnCancel.Location = new Point(400, 12);
            btnCancel.Click += (s, e) => form.Close();
            bottomPanel.Controls.Add(btnCancel);

            Button btnInstall = new Button();
            btnInstall.Text = "Install";
            btnInstall.Font = new Font("Segoe UI", 9f, FontStyle.Bold);
            btnInstall.Size = new Size(100, 32);
            btnInstall.Location = new Point(290, 12);
            btnInstall.BackColor = Color.FromArgb(0, 113, 207);
            btnInstall.ForeColor = Color.White;
            btnInstall.FlatStyle = FlatStyle.Flat;
            btnInstall.FlatAppearance.BorderSize = 0;
            btnInstall.Click += (s, e) =>
            {
                btnInstall.Enabled = false;
                btnCancel.Enabled = false;
                try
                {
                    string installedExe = PerformInstall(chkDesktop.Checked, chkAutostart.Checked);
                    if (chkLaunch.Checked && File.Exists(installedExe))
                    {
                        Process.Start(new ProcessStartInfo(installedExe) { UseShellExecute = true });
                    }
                    MessageBox.Show("FantasTech has been successfully installed!", "Setup Complete", MessageBoxButtons.OK, MessageBoxIcon.Information);
                    form.Close();
                }
                catch (Exception ex)
                {
                    MessageBox.Show("Installation error: " + ex.Message, "Setup Error", MessageBoxButtons.OK, MessageBoxIcon.Error);
                    btnInstall.Enabled = true;
                    btnCancel.Enabled = true;
                }
            };
            bottomPanel.Controls.Add(btnInstall);

            Application.Run(form);
        }

        public static string PerformInstall(bool createDesktop = true, bool createAutostart = true)
        {
            // 1. Terminate running processes to unlock binary
            try
            {
                var p = Process.Start(new ProcessStartInfo("taskkill.exe", "/F /IM fantastech.exe")
                {
                    CreateNoWindow = true,
                    UseShellExecute = false
                });
                p.WaitForExit(2000);
            }
            catch {}

            // 2. Resolve destination directory
            string localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            string installDir = Path.Combine(localAppData, "Programs", AppName);
            string iconsDir = Path.Combine(installDir, "icons");
            Directory.CreateDirectory(installDir);
            Directory.CreateDirectory(iconsDir);

            // 3. Extract executable
            string targetExe = Path.Combine(installDir, "fantastech.exe");
            byte[] compressedBytes = Convert.FromBase64String(CompressedExeData);
            using (var inMs = new MemoryStream(compressedBytes))
            using (var gz = new GZipStream(inMs, CompressionMode.Decompress))
            using (var outFs = new FileStream(targetExe, FileMode.Create, FileAccess.Write))
            {
                gz.CopyTo(outFs);
            }

            // 4. Extract icon
            string iconPath = Path.Combine(iconsDir, "fantastech.ico");
            if (!string.IsNullOrEmpty(IconData))
            {
                byte[] icBytes = Convert.FromBase64String(IconData);
                File.WriteAllBytes(iconPath, icBytes);
            }

            // 5. Create uninstaller script
            string uninstallerBat = Path.Combine(installDir, "uninstall.bat");
            string uninstallScript = "@echo off\r\n" +
                "echo Uninstalling FantasTech...\r\n" +
                "taskkill.exe /F /IM fantastech.exe >nul 2>nul\r\n" +
                "\"" + targetExe + "\" uninstall --purge\r\n" +
                "cd /d \"%TEMP%\"\r\n" +
                "start /b \"\" cmd /c \"ping 127.0.0.1 -n 2 >nul & rd /s /q \\\"" + installDir + "\\\" >nul 2>nul\"\r\n";
            File.WriteAllText(uninstallerBat, uninstallScript);

            // 6. Create Start Menu Shortcut
            string startMenuDir = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.Programs), AppName);
            Directory.CreateDirectory(startMenuDir);
            CreateShortcut(Path.Combine(startMenuDir, "FantasTech.lnk"), targetExe, iconPath, "");
            CreateShortcut(Path.Combine(startMenuDir, "Uninstall FantasTech.lnk"), uninstallerBat, iconPath, "");

            // 7. Create Desktop Shortcut if checked
            if (createDesktop)
            {
                string desktopDir = Environment.GetFolderPath(Environment.SpecialFolder.DesktopDirectory);
                CreateShortcut(Path.Combine(desktopDir, "FantasTech.lnk"), targetExe, iconPath, "");
            }

            // 8. Create Autostart Shortcut if checked
            if (createAutostart)
            {
                string startupDir = Environment.GetFolderPath(Environment.SpecialFolder.Startup);
                CreateShortcut(Path.Combine(startupDir, "FantasTech Service.lnk"), targetExe, iconPath, "service run");
            }

            // 9. Register in Windows Settings "Installed apps" (Add or Remove Programs)
            try
            {
                using (var key = Registry.CurrentUser.CreateSubKey(@"Software\Microsoft\Windows\CurrentVersion\Uninstall\FantasTech"))
                {
                    if (key != null)
                    {
                        key.SetValue("DisplayName", AppName);
                        key.SetValue("DisplayVersion", AppVersion);
                        key.SetValue("Publisher", "Ahmad Serieh");
                        key.SetValue("DisplayIcon", targetExe);
                        key.SetValue("InstallLocation", installDir);
                        key.SetValue("UninstallString", "\"" + uninstallerBat + "\"");
                        key.SetValue("NoModify", 1, RegistryValueKind.DWord);
                        key.SetValue("NoRepair", 1, RegistryValueKind.DWord);
                    }
                }
            }
            catch {}

            return targetExe;
        }

        private static void CreateShortcut(string shortcutPath, string targetPath, string iconPath, string arguments)
        {
            try
            {
                Type shellType = Type.GetTypeFromProgID("WScript.Shell");
                dynamic shell = Activator.CreateInstance(shellType);
                dynamic shortcut = shell.CreateShortcut(shortcutPath);
                shortcut.TargetPath = targetPath;
                shortcut.WorkingDirectory = Path.GetDirectoryName(targetPath);
                if (!string.IsNullOrEmpty(arguments)) shortcut.Arguments = arguments;
                if (!string.IsNullOrEmpty(iconPath) && File.Exists(iconPath)) shortcut.IconLocation = iconPath + ",0";
                shortcut.Save();
            }
            catch {}
        }
    }
}
"@

# Locate C# compiler in .NET Framework directory
$netDir = [System.Runtime.InteropServices.RuntimeEnvironment]::GetRuntimeDirectory()
$cscPath = Join-Path $netDir "csc.exe"

if (-not (Test-Path $cscPath)) {
    # Check 64-bit framework
    $cscPath = "C:\Windows\Microsoft.NET\Framework64\v4.0.30319\csc.exe"
}
if (-not (Test-Path $cscPath)) {
    $cscPath = "C:\Windows\Microsoft.NET\Framework\v4.0.30319\csc.exe"
}

$tempCs = Join-Path ([System.IO.Path]::GetTempPath()) "FantasTechSetup.cs"
[System.IO.File]::WriteAllText($tempCs, $csharpSource)

if (Test-Path $cscPath) {
    Write-Host "Invoking csc.exe: $cscPath" -ForegroundColor DarkGray
    $iconArg = if (Test-Path $IconPath) { "/win32icon:`"$IconPath`"" } else { "" }
    $compileCmd = "& `"$cscPath`" /target:winexe /optimize+ /platform:anycpu $iconArg /out:`"$OutputSetupExe`" `"$tempCs`""
    Invoke-Expression $compileCmd | Out-Null
} else {
    # Compile using Add-Type
    Add-Type -TypeDefinition $csharpSource -Language CSharp -OutputAssembly $OutputSetupExe -OutputType WindowsGUI -ReferencedAssemblies @("System.Windows.Forms", "System.Drawing")
}

Remove-Item -Path $tempCs -Force -ErrorAction SilentlyContinue

if (Test-Path $OutputSetupExe) {
    Write-Host "Created standalone Windows Setup executable: $OutputSetupExe" -ForegroundColor Green
} else {
    Write-Error "Failed to generate $OutputSetupExe"
    exit 1
}
