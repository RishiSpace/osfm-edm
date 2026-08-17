param(
    [Parameter(Mandatory = $true)][string]$Token,
    [string]$Server = "https://localhost:8080",
    [Parameter(Mandatory = $true)][string]$CaFingerprint
)
$agent = Get-Command osfm-edm-agent -ErrorAction SilentlyContinue
if (-not $agent) { throw "osfm-edm-agent not on PATH" }
& osfm-edm-agent --server $Server --token $Token --ca-fingerprint $CaFingerprint
