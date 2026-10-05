param([Parameter(Mandatory=$true)][string]$Executable)
# Native feature-off contract, no GUI automation and no real catalog.
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = $Executable
$info.Arguments = '--raybend-ai-worker'
$info.UseShellExecute = $false
$info.CreateNoWindow = $true
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$info.StandardErrorEncoding = [System.Text.Encoding]::UTF8
$process = New-Object System.Diagnostics.Process
$process.StartInfo = $info
$started = $false
try {
 $null = $process.Start()
 $started = $true
 $stdout = $process.StandardOutput.ReadToEndAsync()
 $stderr = $process.StandardError.ReadToEndAsync()
 $process.StandardInput.Close()
 if (!$process.WaitForExit(10000)) { throw 'Disabled worker did not exit promptly' }
 if ($process.ExitCode -ne 2) { throw "Disabled worker exit differs: $($process.ExitCode)" }
 $expected = [System.Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('5q2k5p6E5bu65pyq5YyF5ZCrIEFJIOivhuWIqw=='))
 if ($stdout.Result.Length -ne 0 -or !$stderr.Result.Contains($expected)) { throw 'Disabled worker did not return the explicit capability error' }
 [Console]::WriteLine((@{exit_code=$process.ExitCode;worker_rejected=$true;output_bytes=$stdout.Result.Length;error=$stderr.Result.Trim()} | ConvertTo-Json -Compress))
} finally {
 if ($started -and !$process.HasExited) { $process.Kill(); $process.WaitForExit() }
 $process.Dispose()
}
