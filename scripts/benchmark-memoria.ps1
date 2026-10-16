# Mede memória e CPU de um app desktop somando TODA a árvore de processos
# (Tauri: app + msedgewebview2; Electron: principal + renderer + GPU + utilitários).
#
# Cada rodada: abre o app a frio, amostra a cada 1 s por $Segundos, fecha
# (CloseMainWindow; encerra o que sobrar após 15 s) e espera $Pausa s.
# Saída: CSV com uma linha por amostra (rodada, app, segundo, memória, CPU).
#
# Uso:
#   .\benchmark-memoria.ps1 -Apps @{ Silex = "C:\...\silex.exe"; Obsidian = "C:\...\Obsidian.exe" } `
#       -Rodadas 3 -Segundos 90 -Saida resultados.csv
param(
  [Parameter(Mandatory)] [hashtable]$Apps,
  [int]$Rodadas = 3,
  [int]$Segundos = 90,
  [int]$Pausa = 20,
  [string]$Saida = "benchmark.csv"
)

function Arvore([int]$raiz) {
  $todos = Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId
  $ids = [System.Collections.Generic.List[int]]::new()
  $ids.Add($raiz)
  for ($i = 0; $i -lt $ids.Count; $i++) {
    foreach ($p in $todos) { if ($p.ParentProcessId -eq $ids[$i] -and $p.ProcessId -ne $ids[$i]) { $ids.Add([int]$p.ProcessId) } }
  }
  $ids
}

function Medir([int]$raiz) {
  $p = @(Get-Process -Id (Arvore $raiz) -ErrorAction SilentlyContinue)
  [pscustomobject]@{
    Processos = $p.Count
    PrivadaMB = [math]::Round(($p | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB, 1)
    WorkingMB = [math]::Round(($p | Measure-Object WorkingSet64 -Sum).Sum / 1MB, 1)
    CpuSeg    = [math]::Round(($p | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum, 3)
  }
}

$linhas = [System.Collections.Generic.List[object]]::new()
$nomes = @($Apps.Keys | Sort-Object)
for ($r = 1; $r -le $Rodadas; $r++) {
  # Ordem alternada a cada rodada, para não favorecer quem roda primeiro (cache de disco)
  $ordem = if ($r % 2) { $nomes } else { $c = $nomes.Clone(); [array]::Reverse($c); $c }
  foreach ($nome in $ordem) {
    $exe = $Apps[$nome]
    if (Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($exe)) -ErrorAction SilentlyContinue) { throw "$nome já está aberto: feche antes de medir." }
    Write-Host "Rodada $r · $nome"
    $proc = Start-Process -FilePath $exe -PassThru
    $t0 = Get-Date
    for ($s = 1; $s -le $Segundos; $s++) {
      Start-Sleep -Milliseconds ([math]::Max(0, ($t0.AddSeconds($s) - (Get-Date)).TotalMilliseconds))
      $m = Medir $proc.Id
      $linhas.Add([pscustomobject]@{ Rodada = $r; App = $nome; Segundo = $s; Processos = $m.Processos; PrivadaMB = $m.PrivadaMB; WorkingMB = $m.WorkingMB; CpuSeg = $m.CpuSeg })
    }
    $ids = Arvore $proc.Id
    $proc.CloseMainWindow() | Out-Null
    $limite = (Get-Date).AddSeconds(15)
    while ((Get-Date) -lt $limite -and (Get-Process -Id $ids -ErrorAction SilentlyContinue)) { Start-Sleep -Milliseconds 500 }
    Get-Process -Id $ids -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Seconds $Pausa
  }
}
$linhas | Export-Csv -Path $Saida -NoTypeInformation -Encoding UTF8
Write-Host "Gravado: $Saida ($($linhas.Count) amostras)"
