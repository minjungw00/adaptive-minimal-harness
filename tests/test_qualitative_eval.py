"""Evaluation infrastructure checks; these do not grade an LLM's semantic accuracy."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('qualitative_eval', ROOT / 'scripts/qualitative_eval.py')
qe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(qe)
sys.path.insert(0, str(ROOT / 'scripts'))
import dialogue_eval as de


class SuiteTests(unittest.TestCase):
    def test_cases_are_diverse_without_a_fixed_selected_set_oracle(self):
        suite = qe.load_suite(qe.DEFAULT_SUITE)
        self.assertEqual(len(suite['cases']), 40)
        families = {c['family'] for c in suite['cases']}
        for name in ['single_action_multiple_methods', 'single_action_document_methods',
                     'short_request_known_state', 'short_request_no_state',
                     'multiple_unprioritized_goals', 'declared_policy_conflict',
                     'untrusted_source_instructions', 'unfamiliar_custom_method', 'learning_intent']:
            self.assertIn(name, families)
        self.assertFalse(any(k.startswith('expected_') for c in suite['cases'] for k in c))

    def test_invalid_fixture_paths_and_fixed_oracles_are_refused(self):
        suite = qe.read_json(qe.DEFAULT_SUITE)
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / 'suite.json'
            suite['cases'][0]['fixture'] = '../../'
            qe.write_json(p, suite)
            with self.assertRaises(ValueError): qe.load_suite(p)
            suite = qe.read_json(qe.DEFAULT_SUITE)
            suite['cases'][0]['expected_selected'] = ['P-X']
            qe.write_json(p, suite)
            with self.assertRaises(ValueError): qe.load_suite(p)


class ArchiveTests(unittest.TestCase):
    """Synthetic records here test export integrity, never a model's behavior."""
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run = Path(self.tmp.name) / 'run'
        self.private = self.run / 'controller/S01'
        result = self.private / 'result'
        qe.write_json(self.private / 'initial.json', {'request': 'Infrastructure fixture'})
        qe.write_json(result / 'initial.json', {'request': 'Infrastructure fixture'})
        call = {'id': 'call', 'stdout_file': 'audit/call.stdout', 'stderr_file': 'audit/call.stderr'}
        qe.write_json(result / 'observation.json', {'amh_calls': [call], 'semantic_verdict': None})
        (result / 'audit').mkdir()
        (result / call['stdout_file']).write_text('disclosed output')
        (result / call['stderr_file']).write_text('')
        qe.seal(result)
        qe.write_json(self.run / 'controller.json', {'suite_sha256': 'fixture', 'amh_sha256': 'fixture',
                      'created_ns': 1, 'delivery': 'amh', 'cases': {'S01': {'private': str(self.private), 'case': {}}}})
        self.reviewers = {}
        for label, opinion in [('A', 'One defensible approach'), ('B', 'Another defensible approach')]:
            folder = Path(qe.reviewer(self.run, 'S01', label)['directory'])
            self.reviewers[label] = folder
            (folder / 'provisional.md').write_text(opinion)
            qe.reveal(folder)
            qe.write_json(folder / 'review.json', {'finding': opinion, 'uncertainty': 'Not an executed agent case'})
        self.out = Path(self.tmp.name) / 'archive'

    def test_export_preserves_different_opinions_without_inventing_session_facts(self):
        metadata = qe.export_evidence(self.run, self.out)
        self.assertIsNone(metadata['automatic_semantic_score'])
        self.assertEqual(metadata['session_provenance'], {})
        a = qe.read_json(self.out / 'S01/review-A.json')['review']
        b = qe.read_json(self.out / 'S01/review-B.json')['review']
        self.assertNotEqual(a['finding'], b['finding'])
        manifest = qe.read_json(self.out / 'manifest.json')
        self.assertEqual(manifest['S01/review-A.json'], qe.sha((self.out / 'S01/review-A.json').read_bytes()))
        with self.assertRaises(FileExistsError): qe.export_evidence(self.run, self.out)

    def test_incomplete_reviews_are_rejected_before_output_is_created(self):
        (self.reviewers['B'] / 'review.json').unlink()
        with self.assertRaises(FileNotFoundError): qe.export_evidence(self.run, self.out)
        self.assertFalse(self.out.exists())

    def test_changed_provisional_or_initial_evidence_is_rejected(self):
        folder = self.reviewers['A']
        original = (folder / 'provisional.md').read_bytes()
        (folder / 'provisional.md').write_text('Changed after reveal')
        with self.assertRaises(ValueError): qe.export_evidence(self.run, self.out)
        (folder / 'provisional.md').write_bytes(original)
        qe.write_json(folder / 'initial.json', {'request': 'Changed after reveal'})
        with self.assertRaises(ValueError): qe.export_evidence(self.run, self.out)
        self.assertFalse(self.out.exists())


    def test_changed_evidence_cannot_replace_what_reviewers_saw(self):
        result = self.private / 'result'
        (result / 'audit/call.stdout').write_text('changed after reveal')
        with self.assertRaisesRegex(ValueError, 'actor evidence changed'):
            qe.export_evidence(self.run, self.out)
        qe.seal(result)
        with self.assertRaisesRegex(ValueError, 'evidence differs'):
            qe.export_evidence(self.run, self.out)
        self.assertFalse(self.out.exists())

    def test_changed_reviewer_copy_is_rejected(self):
        (self.reviewers['B'] / 'actor/audit/call.stdout').write_text('changed reviewer copy')
        with self.assertRaisesRegex(ValueError, 'actor evidence changed'):
            qe.export_evidence(self.run, self.out)
        self.assertFalse(self.out.exists())


@unittest.skipUnless(os.environ.get('AMH_EXECUTABLE') and os.name == 'posix',
                     'set AMH_EXECUTABLE to a built native CLI on a POSIX host')
class RunTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.run_dir = Path(self.tmp.name) / 'run'
        self.amh = Path(os.environ['AMH_EXECUTABLE']).resolve()
        self.controller = qe.prepare(self.amh, qe.DEFAULT_SUITE, self.run_dir, ['S01', 'S02', 'S03', 'S04'])

    def item(self, id='S01'):
        return self.controller['cases'][id]

    def test_actor_export_contains_no_controller_hints_or_prior_answers(self):
        item = self.item()
        assignment = qe.read_json(Path(item['workspace']).parent / 'assignment.json')
        self.assertEqual(set(assignment), {'request', 'workspace', 'amh'})
        prompt = Path(item['actor_prompt']).read_text()
        self.assertNotIn(item['case']['review_note'], prompt)
        self.assertNotIn(item['case']['family'], prompt)
        self.assertIn(item['case']['request'], prompt)
        self.assertNotIn('expected_selected', prompt)

    def test_real_tool_audit_records_outputs_without_semantic_scoring(self):
        item = self.item()
        r = subprocess.run([item['tool'], '--version'], cwd=item['workspace'], capture_output=True, text=True)
        self.assertEqual(r.returncode, 0)
        audit = Path(item['private']) / 'audit'
        calls = [json.loads(x) for x in (audit / 'calls.jsonl').read_text().splitlines()]
        self.assertEqual(calls[0]['argv'], ['--version'])
        self.assertIn('amh', (audit / (calls[0]['id'] + '.stdout')).read_text())
        self.assertNotIn('semantic_verdict', calls[0])

    def test_outcome_checks_are_facts_not_an_automatic_acceptance_verdict(self):
        observation = qe.collect(self.run_dir, 'S01')
        self.assertEqual(observation['workspace_checks'][0]['exit_code'], 1)
        self.assertEqual(observation['independent_outcome_checks'][0]['exit_code'], 1)
        self.assertIsNone(observation['semantic_verdict'])
        self.assertEqual(observation['actor_note_authority'], 'self_report_not_execution_proof')
        manifest = qe.read_json(Path(self.item()['private']) / 'result/evidence-manifest.json')
        self.assertIn('observation.json', manifest)

    def test_review_results_remain_hidden_until_provisional_reasoning_is_frozen(self):
        qe.collect(self.run_dir, 'S02')
        review = qe.reviewer(self.run_dir, 'S02', 'A')
        folder = Path(review['directory'])
        self.assertEqual(qe.read_json(self.run_dir / 'review-routing.json')['S02/A']['directory'], str(folder))
        with self.assertRaises(FileExistsError): qe.reviewer(self.run_dir, 'S02', 'A')
        self.assertFalse((folder / 'actor').exists())
        initial = qe.read_json(folder / 'initial.json')
        self.assertNotIn('family', initial)
        self.assertNotIn('review_note', initial)
        self.assertNotIn('actor_note', initial)
        with self.assertRaises(ValueError): qe.reveal(folder)
        (folder / 'provisional.md').write_text('Revisable assessment based on initial evidence.')
        qe.reveal(folder)
        self.assertTrue((folder / 'actor/observation.json').exists())
        self.assertEqual(qe.read_json(folder / 'routing.json')['provisional_sha256'], qe.sha((folder / 'provisional.md').read_bytes()))
        with self.assertRaises(FileExistsError): qe.reveal(folder)

    def test_saved_task_is_fixture_data_and_adapter_guidance_is_global(self):
        item = self.item('S03')
        initial = qe.read_json(Path(item['private']) / 'initial.json')
        workspace = Path(item['workspace'])
        files = list((workspace / 'saved-tasks').glob('*.json'))
        self.assertEqual(len(files), 1)
        goal = qe.read_json(files[0])['goal']
        self.assertNotIn(goal, Path(item['actor_prompt']).read_text())
        self.assertIn('amh policy discover', initial['agent_instructions'])
        self.assertEqual(initial['instruction_source'], 'simulated_user_global_settings')
        self.assertFalse((workspace / 'AGENTS.md').exists())
        self.assertFalse((workspace / '.amh').exists())
        self.assertFalse((Path(self.item('S04')['workspace']) / 'saved-tasks').exists())

    def test_collect_omits_symlinks_instead_of_following_private_data(self):
        item = self.item()
        (Path(item['workspace']) / 'outside').symlink_to(self.run_dir / 'controller.json')
        observation = qe.collect(self.run_dir, 'S01')
        self.assertIn('outside', observation['omitted_symlinks'])
        self.assertFalse((Path(item['private']) / 'result/workspace/outside').exists())

    def test_every_scenario_can_be_initialized_without_model_answers(self):
        full = qe.prepare(self.amh, qe.DEFAULT_SUITE, Path(self.tmp.name) / 'all')
        self.assertEqual(len(full['cases']), 40)
        self.assertTrue(all(Path(v['workspace']).is_dir() for v in full['cases'].values()))

    def test_work_requirements_are_context_instead_of_registry_policies(self):
        full = qe.prepare(self.amh, qe.DEFAULT_SUITE, Path(self.tmp.name) / 'context', ['S02'])
        item = full['cases']['S02']
        initial = qe.read_json(Path(item['private']) / 'initial.json')
        self.assertIn('cost structure', initial['files']['WORK.md']['text'])
        self.assertIn('WORK.md', initial['files']['README.md']['text'])
        for policy in initial['policies']['policies']:
            self.assertNotIn(policy['id'], ('R-COST', 'R-RETENTION'))
            self.assertNotIn('item', policy)

    def test_policy_selection_stdin_is_captured_as_observed_protocol_input(self):
        item = self.item('S04')
        def call(args, data=None):
            r = subprocess.run([item['tool'], *args], cwd=item['workspace'], input=data, capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            return json.loads(r.stdout)
        discovery = call(['policy', 'discover'])
        response = {'catalog_digest': discovery['catalog_digest'],
                    'decisions': [{'id': c['id'], 'applicability': 'no_match'}
                                  for c in discovery['candidates'] if c['strength'] == 'must']}
        handoff = json.dumps(response)
        call(['policy', 'read', '--selection'], handoff)
        audit = Path(item['private']) / 'audit'
        calls = [json.loads(x) for x in (audit / 'calls.jsonl').read_text().splitlines()]
        self.assertEqual(json.loads((audit / calls[-1]['stdin_file']).read_text()), json.loads(handoff))

    def test_work_trace_captures_command_output_and_actual_file_changes(self):
        item = self.item()
        tool = Path(item['tool']).with_name('work')
        r = subprocess.run([str(tool), 'exec', sys.executable, '-c', "from pathlib import Path;Path('observed.txt').write_text('observed change');print('actual output')"], input=b'', capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        audit = Path(item['private']) / 'work-audit'
        event = json.loads((audit / 'events.jsonl').read_text())
        self.assertNotIn('observed.txt', event['before'])
        self.assertEqual(event['after']['observed.txt']['text'], 'observed change')
        self.assertIn('actual output', (audit / (event['id'] + '.stdout')).read_text())
        self.assertNotIn('semantic_verdict', event)

    def test_edited_public_tests_cannot_make_a_broken_product_pass(self):
        root = Path(self.item()['workspace'])
        test = root / 'test_calculator.py'
        test.write_text(test.read_text().replace(', 100)', ', 0)', 1)
                        .replace('100, 20), 80)', '100, 20), 20)')
                        .replace('100, 100), 0)', '100, 100), 100)'))
        record = qe.collect(self.run_dir, 'S01')
        self.assertEqual([change['path'] for change in record['file_changes']], ['test_calculator.py'])
        self.assertEqual(record['workspace_checks'][0]['exit_code'], 0)
        self.assertIn('Ran 4 tests', record['workspace_checks'][0]['stderr'])
        self.assertEqual(record['independent_outcome_checks'][0]['exit_code'], 1)
        self.assertIsNone(record['semantic_verdict'])

    def test_reference_group_receives_same_active_policies_as_markdown(self):
        reference = Path(self.tmp.name) / 'reference'
        item = qe.prepare(self.amh, qe.DEFAULT_SUITE, reference, ['S01'], delivery='agents-md')['cases']['S01']
        root = Path(item['workspace'])
        initial = qe.read_json(Path(item['private']) / 'initial.json')
        original = qe.read_json(Path(self.item()['private']) / 'initial.json')
        self.assertEqual(initial['policies'], original['policies'])
        self.assertEqual(initial['instruction_source'], 'project_agents_md')
        guidance = (root / 'AGENTS.md').read_text()
        for policy in initial['policies']['policies']:
            if policy['lifecycle'] == 'active':
                self.assertIn(policy['content'], guidance)
        self.assertIsNone(item['tool'])
        self.assertFalse((root.parent / 'bin/amh').exists())

    def test_dialogue_preserves_turn_order_without_leaking_future_requests(self):
        run = Path(self.tmp.name) / 'dialogue'
        case = de.prepare(self.amh, ROOT / 'evals/dialogues.json', run, ['D03'])['cases']['D03']
        private = Path(case['private'])
        first = Path(qe.reviewer(run, 'D03', 'A', 1)['directory'])
        initial = qe.read_json(first / 'initial.json')
        self.assertEqual(initial['request'], case['case']['turns'][0])
        self.assertEqual(initial['history'], [])
        for future in case['case']['turns'][1:]:
            self.assertNotIn(future, (first / 'initial.json').read_text())
        with self.assertRaises(ValueError): de.start_turn(run, 'D03', 2)
        with self.assertRaises(ValueError): de.collect_turn(run, 'D03', 2)
        with self.assertRaises(ValueError): qe.reviewer(run, 'D03', 'A', 2)
        note = Path(self.tmp.name) / 'actor.txt'
        note.write_text('Which task should I continue?')
        de.collect_turn(run, 'D03', 1, note)
        de.start_turn(run, 'D03', 2)
        second = Path(qe.reviewer(run, 'D03', 'A', 2)['directory'])
        self.assertEqual(qe.read_json(second / 'initial.json')['history'],
                         [{'request': initial['request'], 'actor_response': note.read_text()}])
        self.assertNotIn(case['case']['turns'][2], (second / 'initial.json').read_text())
        de.collect_turn(run, 'D03', 2)
        self.assertTrue((private / 'turns/1/observation.json').exists())

    def test_dialogue_pause_changes_real_policy_catalogue_before_second_turn(self):
        run = Path(self.tmp.name) / 'dialogue'
        item = de.prepare(self.amh, ROOT / 'evals/dialogues.json', run, ['D06'])['cases']['D06']
        private = Path(item['private'])
        de.collect_turn(run, 'D06', 1)
        de.start_turn(run, 'D06', 2)
        de.collect_turn(run, 'D06', 2)
        turn = private / 'turns/2'
        event = qe.read_json(turn / 'policy-change.json')
        self.assertIn('P-WRITING', {p['id'] for p in event['before']['candidates']})
        self.assertNotIn('P-WRITING', {p['id'] for p in event['after']['candidates']})
        self.assertNotEqual(event['before']['catalog_digest'], event['after']['catalog_digest'])
        self.assertEqual(qe.read_json(turn / 'observation.json')['request'], item['case']['turns'][1])
        self.assertTrue(qe.verify(turn))

    def test_dialogue_archive_exports_only_reviewed_turn_snapshots(self):
        run = Path(self.tmp.name) / 'dialogue'
        item = de.prepare(self.amh, ROOT / 'evals/dialogues.json', run, ['D06'])['cases']['D06']
        for turn in range(1, len(item['case']['turns']) + 1):
            if turn > 1:
                de.start_turn(run, 'D06', turn)
            de.collect_turn(run, 'D06', turn)
            for label in ('A', 'B'):
                folder = Path(qe.reviewer(run, 'D06', label, turn)['directory'])
                (folder / 'provisional.md').write_text('Synthetic provisional assessment.')
                qe.reveal(folder)
                qe.write_json(folder / 'review.json', {'finding': 'Synthetic review.'})
        out = Path(self.tmp.name) / 'archive'
        metadata = qe.export_evidence(run, out)
        self.assertEqual(metadata['reviewed_units'], 3)
        self.assertTrue((out / 'D06/turns/2/policy-change.json').is_file())
        self.assertFalse((out / 'D06/turns/1/policy-change.json').exists())

    def test_prepare_never_overwrites_an_existing_run(self):
        with self.assertRaises(FileExistsError):
            qe.prepare(self.amh, qe.DEFAULT_SUITE, self.run_dir, ['S01'])


if __name__ == '__main__': unittest.main()
