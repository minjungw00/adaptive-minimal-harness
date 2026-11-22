# AMH task: TASK-6207c38f0c6440c5

Goal: 이 함수를 정리해줘.

Natural-language request fingerprint: 977714d13e18a888366bcf6a9b2981220fbf0fcf6b7e3700894f7c9c78f547cf
Applicability and completion criteria were interpreted by an agent; they are not independent semantic verification or approval.

Done when:
- label has a clearer internal structure with duplicated return assembly removed.
- The label(first, last) signature and existing observable behavior, including truthiness before stripping, are preserved.
- Run python -m unittest -v and report actual results.

## P-API · principle · revision 2

Preserve supported API compatibility or explicitly document and authorize the breaking change.

Source: owner=user; scope=Project; provenance=pack:development@0.2.0
Strength: Must

Verification:
- Check affected callers and compatibility expectations.

## P-REFACTOR · principle · revision 2

Preserve observable behavior while improving structure. Reclassify intended behavior changes before proceeding.

Source: owner=user; scope=Project; provenance=pack:development@0.2.0
Strength: Must

Verification:
- Compare relevant behavior before and after the refactor.

Report only checks actually executed. Policy delivery is not evidence of compliance.
