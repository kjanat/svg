# Use literal AST words so tab completion never evaluates the command line.
Register-ArgumentCompleter -Native -CommandName @(
    'svg-format', 'svg-lint', 'svg-language-server',
    'svg-format.exe', 'svg-lint.exe', 'svg-language-server.exe'
) -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)

    $elements = $commandAst.CommandElements
    $command = $commandAst.GetCommandName()
    if (-not $command) { return }

    $words = @($command)
    $hasCurrentWord = $false
    foreach ($element in ($elements | Select-Object -Skip 1)) {
        if ($element.Extent.StartOffset -ge $cursorPosition) { break }
        if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) {
            if ($element.Extent.EndOffset -gt $cursorPosition) {
                $words += $wordToComplete
            } else {
                $words += $element.Value
            }
        } elseif ($element -is [System.Management.Automation.Language.CommandParameterAst]) {
            $words += $element.Extent.Text.Substring(0, [Math]::Min(
                $element.Extent.Text.Length, $cursorPosition - $element.Extent.StartOffset))
        } else {
            return
        }
        $hasCurrentWord = $element.Extent.EndOffset -ge $cursorPosition -and $wordToComplete -ne ''
    }
    if (-not $hasCurrentWord) { $words += $wordToComplete }

    # The Fish protocol emits one candidate per line with an optional tab
    # and description. Both shells complete the final word of the request.
    $previous = $env:SVG_COMPLETE
    try {
        $env:SVG_COMPLETE = 'fish'
        & $command -- @words | ForEach-Object {
            $candidate, $description = $_ -split "`t", 2
            if (-not $candidate) { return }
            $text = if ($candidate -match '[^\p{L}\p{N}_./:\\=-]') {
                "'" + $candidate.Replace("'", "''") + "'"
            } else {
                $candidate
            }
            if (-not $description) { $description = $candidate }
            [System.Management.Automation.CompletionResult]::new(
                $text, $candidate, 'ParameterValue', $description)
        }
    } finally {
        $env:SVG_COMPLETE = $previous
    }
}
