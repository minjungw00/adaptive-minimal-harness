# AMH task: TASK-b4d6a80729709de2

Goal: 이 오류를 고쳐줘.

Natural-language request fingerprint: 741e079d9739e5b8d68c2f2e5a98f5cc8c99b648884e38a4ef6e543891c9f59a
Applicability and completion criteria were interpreted by an agent; they are not independent semantic verification or approval.

Done when:
- Reproduce the percentage-discount calculation failure with automated tests before editing the implementation.
- Correct the payable total while preserving discounted_total(price, percent) and invalid-input ValueError behavior.
- Run the regression tests and report observed results.

## P-API · principle · revision 2

Preserve supported API compatibility or explicitly document and authorize the breaking change.

Source: owner=user; scope=Project; provenance=pack:development@0.2.0
Strength: Must

Verification:
- Check affected callers and compatibility expectations.

## P-DEBUG · principle · revision 2

Reproduce the failure and test a concrete cause before changing code.

Source: owner=user; scope=Project; provenance=pack:development@0.2.0
Strength: Should

Verification:
- Record reproduction and the check that distinguishes the proposed cause.

## P-REGRESSION · principle · revision 2

Add a regression check at the layer where the defect occurred.

Source: owner=user; scope=Project; provenance=pack:development@0.2.0
Strength: Should

Verification:
- Show the check detects the original failure.

## P-TDD · principle · revision 2

Use Red–Green–Refactor for testable behavior changes. Demonstrate the failing test before implementation, then passing tests.

Source: owner=user; scope=Project; provenance=pack:development@0.2.0
Strength: Must

Verification:
- Record the failing-before and passing-after commands and results.

## R-API · requirement · revision 2

Preserve the public discounted_total(price, percent) interface and its invalid-input ValueError behavior.

Source: owner=user; scope=Project; provenance=Accepted project requirement
Requirement role: Core

Acceptance criteria:
- Preserve the public discounted_total(price, percent) interface and its invalid-input ValueError behavior.

Report only checks actually executed. Policy delivery is not evidence of compliance.
