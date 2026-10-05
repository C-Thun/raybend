param(
 [Parameter(Mandatory=$true)][string]$Executable,
 [Parameter(Mandatory=$true)][string]$ResourceDirectory,
 [Parameter(Mandatory=$true)][string[]]$Images
)
# Full desktop executable in worker mode: no window, no real catalog, no GUI E2E.
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$info = New-Object System.Diagnostics.ProcessStartInfo
$info.FileName = $Executable
$info.Arguments = '--raybend-ai-worker'
$info.UseShellExecute = $false
$info.RedirectStandardInput = $true
$info.RedirectStandardOutput = $true
$info.RedirectStandardError = $true
$process = New-Object System.Diagnostics.Process
$process.StartInfo = $info
$null = $process.Start()
$errorOutput = $process.StandardError.ReadToEndAsync()
$inputStream = $process.StandardInput.BaseStream
$outputStream = $process.StandardOutput.BaseStream
function Read-Exact([int]$length) {
 $buffer = New-Object byte[] $length
 $offset = 0
 while ($offset -lt $length) {
  $read = $outputStream.ReadAsync($buffer,$offset,$length-$offset)
  if (!$read.Wait(30000)) { throw 'AI worker response timed out' }
  if ($read.Result -eq 0) { throw 'AI worker closed its response stream' }
  $offset += $read.Result
 }
 return ,$buffer
}
function Exchange($request) {
 $payload = [System.Text.Encoding]::UTF8.GetBytes(($request | ConvertTo-Json -Depth 8 -Compress))
 $header = [BitConverter]::GetBytes([uint32]$payload.Length)
 $inputStream.Write($header,0,$header.Length)
 $inputStream.Write($payload,0,$payload.Length)
 $inputStream.Flush()
 $header = Read-Exact 4
 $length = [BitConverter]::ToUInt32($header,0)
 if ($length -eq 0 -or $length -gt 65536) { throw 'Unbounded AI worker response' }
 $payload = Read-Exact $length
 $response = [System.Text.Encoding]::UTF8.GetString($payload) | ConvertFrom-Json
 if ($response.version -ne 1) { throw 'AI worker protocol version differs' }
 return $response
}
$load = [System.Diagnostics.Stopwatch]::StartNew()
try {
 $response = Exchange @{version=1;op='load';library=(Join-Path $ResourceDirectory 'ai-runtime\onnxruntime.dll');model=(Join-Path $ResourceDirectory 'ai-model\image_encoder.onnx');raw=$false}
 if (!$response.ok) { throw $response.error }
 $load.Stop()
 $response = Exchange @{version=2;op='probe';raw=$false}
 if ($response.ok -or $response.features.Count -ne 0) { throw 'Version mismatch accepted' }
 $response = Exchange @{version=1;op='encode';path=(Join-Path $ResourceDirectory 'missing-ai-smoke.jpg');resize='center_crop';raw=$false}
 if ($response.ok -or $response.features.Count -ne 0) { throw 'Missing original returned fake success' }
 foreach ($image in $Images) {
  $timer = [System.Diagnostics.Stopwatch]::StartNew()
  $response = Exchange @{version=1;op='encode';path=$image;resize='center_crop';raw=$false}
  $timer.Stop()
  if (!$response.ok) { throw $response.error }
  if ($response.features.Count -ne 512) { throw 'Image feature shape differs' }
  foreach ($value in $response.features) {
   if ([double]::IsNaN($value) -or [double]::IsInfinity($value)) { throw 'Non-finite image features' }
  }
  [Console]::WriteLine((@{image=$image;feature_count=$response.features.Count;original_preprocess_encode_ms=$timer.Elapsed.TotalMilliseconds} | ConvertTo-Json -Compress))
 }
 $process.Refresh()
 $peak = $process.PeakWorkingSet64
 $inputStream.Close()
 if (!$process.WaitForExit(10000)) { throw 'AI worker did not exit after closing input' }
 if ($process.ExitCode -ne 0) { throw "AI worker exit: $($process.ExitCode)" }
 [Console]::WriteLine((@{load_includes_contract_probe_ms=$load.Elapsed.TotalMilliseconds;peak_working_set_bytes=$peak;exit_code=$process.ExitCode;worker_pid=$process.Id;worker_exited=$process.HasExited} | ConvertTo-Json -Compress))
 [Console]::Error.WriteLine($errorOutput.Result)
} finally {
 if (!$process.HasExited) { $process.Kill();$process.WaitForExit() }
 $process.Dispose()
}
