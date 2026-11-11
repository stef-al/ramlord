# Checks the exe's resources (SPEC.md, M6): its version block names
# RAMlord at the version in Cargo.toml, and it carries the app icon.
#
#   pwsh .github/scripts/exe-resources.ps1 target/release/ramlord.exe 0.1.0
param(
  [Parameter(Mandatory)] [string] $Exe,
  [Parameter(Mandatory)] [string] $Version
)

$path = (Resolve-Path $Exe).Path
$info = (Get-Item $path).VersionInfo
if ($info.ProductVersion -ne $Version -or $info.FileDescription -ne "RAMlord") {
  Write-Output "::error title=Exe resources::the version block says '$($info.FileDescription)' '$($info.ProductVersion)', expected 'RAMlord' '$Version'"
  exit 1
}

# With index -1 and no arrays, ExtractIconExW counts the icons in the file.
Add-Type -Namespace RAMlord -Name Shell -MemberDefinition @'
[DllImport("shell32.dll", CharSet = CharSet.Unicode)]
public static extern uint ExtractIconExW(string file, int index, IntPtr[] large, IntPtr[] small, uint count);
'@
$icons = [RAMlord.Shell]::ExtractIconExW($path, -1, $null, $null, 0)
if ($icons -lt 1) {
  Write-Output "::error title=Exe resources::$Exe has no icon"
  exit 1
}
Write-Output "::notice title=Exe resources::$($info.ProductName) $($info.ProductVersion), $($info.LegalCopyright), $icons icon"
