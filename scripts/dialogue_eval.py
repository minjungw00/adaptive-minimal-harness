#!/usr/bin/env python3
"""Prepare and collect successive turns of the same external agent conversation."""
import argparse
import json
from pathlib import Path
import shutil
import time

import qualitative_eval as qe


def prepare(amh, suite, out, case_ids=None):
    for case in qe.load_suite(suite)['cases']:
        if not case.get('turns') or case['turns'][0] != case['request'] or not all(m.strip() for m in case['turns']):
            raise ValueError('dialogue must start with the assigned request and contain nonempty turns')
        for turn, policies in case.get('pause_before_turn', {}).items():
            number = int(turn)
            if (str(number) != turn or not 2 <= number <= len(case['turns'])
                    or not policies or len(set(policies)) != len(policies)):
                raise ValueError('invalid between-turn pause schedule')
    controller = qe.prepare(amh, suite, out, case_ids)
    for id, item in controller['cases'].items():
        root = Path(item['workspace'])
        for name in item['case'].get('extra_fixtures', []):
            shutil.copytree(qe.REPO / 'evals/fixtures' / name, root, dirs_exist_ok=True)
        for task in item['case'].get('saved_tasks', []):
            qe.write_json(root / 'saved-tasks' / (task['id'] + '.json'), task)
        if item['case'].get('saved_tasks'):
            with (root / 'README.md').open('a', encoding='utf-8') as readme:
                readme.write('\nRecorded work requests are in saved-tasks/. Their existence does not select the current request.\n')
        prompt = Path(item['actor_prompt'])
        prompt.write_text(prompt.read_text(encoding='utf-8') + '\n\n'
                          'Wait after handling this request. Further requests may follow in the same conversation.\n',
                          encoding='utf-8')
        start_turn(out, id, 1)
    return controller


def pause_policies(amh, root, home, policies):
    before = qe.invoke(amh, root, home, 'policy', 'discover')
    candidates = {candidate['id'] for candidate in before.get('candidates', [])}
    if not set(policies) <= candidates:
        raise ValueError('scheduled policies must be active candidates before the pause')
    started = time.time_ns()
    changes = [qe.invoke(amh, root, home, 'policy', 'change', policy, '--pause',
                         '--reason', 'Evaluation intervention between user requests', '--apply')
               for policy in policies]
    after = qe.invoke(amh, root, home, 'policy', 'discover')
    remaining = {candidate['id'] for candidate in after.get('candidates', [])}
    remaining.update(after.get('selected', []))
    if set(policies) & remaining or after.get('catalog_digest') == before['catalog_digest']:
        raise ValueError('scheduled pause did not change the current catalogue')
    return {'started_ns': started, 'finished_ns': time.time_ns(), 'policies': policies,
            'before': before, 'changes': changes, 'after': after}


def start_turn(run, case, turn):
    controller = qe.read_json(Path(run).resolve() / 'controller.json')
    item = controller['cases'][case]
    private, root = Path(item['private']), Path(item['workspace'])
    turns = item['case']['turns']
    destination, _ = qe.evidence_paths(item, turn)
    if destination.exists():
        raise FileExistsError('turn already started')
    history = []
    for previous in range(1, turn):
        _, snapshot = qe.evidence_paths(item, previous)
        qe.verify(snapshot)
        observation = qe.read_json(snapshot / 'observation.json')
        history.append({'request': observation['request'], 'actor_response': observation['actor_note']})
    home = private / 'amh-home'
    amh = Path(controller['amh'])
    if qe.sha(amh.read_bytes()) != controller['amh_sha256']:
        raise ValueError('AMH executable changed after preparing the run')
    if policies := item['case'].get('pause_before_turn', {}).get(str(turn)):
        change_record = destination.with_name(f'{turn}-change.json')
        if not change_record.is_file():
            qe.write_json(change_record, pause_policies(amh, root, home, policies))
        elif qe.read_json(change_record)['after'] != qe.invoke(amh, root, home, 'policy', 'discover'):
            raise ValueError('current catalogue differs from the recorded pause')
    initial = qe.read_json(private / 'initial.json')
    initial.update(request=turns[turn - 1], history=history, files=qe.product_files(root),
                   policies=qe.invoke(amh, root, home, 'policy', 'list', '--full'))
    qe.write_json(destination, initial)
    return turns[turn - 1]


def collect_turn(run, case, turn, actor_note=None):
    return qe.collect(run, case, actor_note, turn=turn)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    prepare_args = commands.add_parser('prepare')
    prepare_args.add_argument('--amh', required=True)
    prepare_args.add_argument('--suite', default=str(qe.REPO / 'evals/dialogues.json'))
    prepare_args.add_argument('--out', required=True)
    prepare_args.add_argument('--cases', nargs='+')
    for command in ('start', 'collect'):
        options = commands.add_parser(command)
        options.add_argument('--run', required=True)
        options.add_argument('--case', required=True)
        options.add_argument('--turn', type=int, required=True)
        if command == 'collect':
            options.add_argument('--actor-note')
    args = parser.parse_args()
    if args.command == 'prepare':
        controller = prepare(args.amh, args.suite, args.out, args.cases)
        print(json.dumps({id: item['actor_prompt'] for id, item in controller['cases'].items()}))
    elif args.command == 'start':
        print(start_turn(args.run, args.case, args.turn))
    else:
        observation = collect_turn(args.run, args.case, args.turn, args.actor_note)
        print(json.dumps({'turn': args.turn, 'calls': len(observation['amh_calls'])}))


if __name__ == '__main__':
    main()
