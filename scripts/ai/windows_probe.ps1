param(
 [Parameter(Mandatory=$true)][string]$Executable,
 [Parameter(Mandatory=$true)][string]$DataDirectory
)
# Two fixed public smoke samples + shared worker shutdown/peak-memory probe.
# DataDirectory contains the fixed model, probe.json, parity tensors, CPU DLL and samples.
$taskExe = $Executable
$taskDir = $DataDirectory
$taskInfo = New-Object System.Diagnostics.ProcessStartInfo
$taskInfo.FileName = $taskExe
$taskInfo.Arguments = '"'+$taskDir+'\onnxruntime.dll" "'+$taskDir+'" "'+$taskDir+'\samples\astronaut.jpg" "'+$taskDir+'\samples\rocket.jpg"'
$taskInfo.UseShellExecute = $false
$taskInfo.RedirectStandardOutput = $true
$taskInfo.RedirectStandardError = $true
$taskProcess = New-Object System.Diagnostics.Process
$taskProcess.StartInfo = $taskInfo
$null = $taskProcess.Start()
$taskOutput = $taskProcess.StandardOutput.ReadToEndAsync()
$taskError = $taskProcess.StandardError.ReadToEndAsync()
$taskParentPeak = [long]0
$taskChildPeak = [long]0
$taskChildPids = @{}
while (!$taskProcess.WaitForExit(10)) {
 $taskProcess.Refresh()
 $taskParentPeak = [Math]::Max($taskParentPeak,$taskProcess.PeakWorkingSet64)
 Get-Process ai-probe -ErrorAction SilentlyContinue | Where-Object {$_.Id -ne $taskProcess.Id} | ForEach-Object {
  $taskChildPids[$_.Id] = $true
  $taskChildPeak = [Math]::Max($taskChildPeak,$_.PeakWorkingSet64)
 }
}
[Console]::WriteLine($taskOutput.Result)
[Console]::Error.WriteLine($taskError.Result)
$taskOrphans = @($taskChildPids.Keys | Where-Object {Get-Process -Id $_ -ErrorAction SilentlyContinue})
[Console]::WriteLine(([pscustomobject]@{parent_peak_working_set_bytes=$taskParentPeak; child_peak_working_set_bytes=$taskChildPeak; observed_children=$taskChildPids.Count; orphan_count=$taskOrphans.Count; exit_code=$taskProcess.ExitCode} | ConvertTo-Json -Compress))
exit $taskProcess.ExitCode
