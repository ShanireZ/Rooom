function Assert-PassiveFocus($Frame) {
    $proof = $Frame.focusProof
    if ($null -eq $proof -or -not $Frame.hwnd -or $null -eq $Frame.foreground) {
        throw 'The passive-focus observation is missing.'
    }
    $phases = @('built', 'before-show', 'after-show', 'render')
    if ($Frame.snapshot.surface -ceq 'panel') { $phases += @('before-reveal', 'after-reveal') }
    foreach ($phase in $phases) {
        if (@($proof.samples | Where-Object { $_.phase -ceq $phase }).Count -eq 0) {
            throw "The passive-focus observation is missing $phase."
        }
    }
    if ($null -eq $proof.ownedForegroundObservations -or
        $null -eq $proof.focusableObservations -or
        $proof.ownedForegroundObservations -ne 0 -or $proof.focusableObservations -ne 0 -or
        $Frame.hwnd -eq $Frame.foreground -or
        @($proof.samples | Where-Object { -not $_.noActivate -or $_.foreground -eq $Frame.hwnd }).Count) {
        throw ("Rooom activated or became focusable during a passive arrival. Foreground observations={0}; focusable observations={1}; currently foreground={2}." -f
            $proof.ownedForegroundObservations, $proof.focusableObservations, ($Frame.hwnd -eq $Frame.foreground))
    }
}
