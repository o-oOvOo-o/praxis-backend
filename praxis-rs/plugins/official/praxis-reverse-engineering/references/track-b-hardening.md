# Track B: Owned Hardening

Track B hardens binaries or shaders owned by the user. It is for IP protection, watermarking, tamper evidence, symbol hygiene, and defensive build changes.

Required flow:

1. Authorization note must state ownership or explicit permission.
2. Scope must include `harden`; hardening must fail closed without that action.
3. Use Track A inspection tools before and after hardening to measure effects.
4. Store results as neutral evidence records and remediation notes.

Allowed hardening families:

- watermarking owned assets
- symbol hygiene and debug-info policy
- tamper detection for self-owned binaries
- defensive obfuscation that does not add malware-like stealth, AV evasion, anti-forensics, or analysis-killing behavior

Disallowed hardening families:

- malware stealth, anti-analysis booby traps, AV evasion, credential protection bypass, platform restriction bypass, license bypass, DRM bypass, or anti-cheat bypass
