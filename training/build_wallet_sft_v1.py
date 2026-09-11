"""Author wallet training scenarios without reading development questions."""
import json
from pathlib import Path
import random

NAMES = ['Ada', 'Bruno', 'Cora', 'Dion', 'Esme', 'Farah', 'Gus', 'Hugo', 'Ida', 'Jin', 'Kira', 'Leon', 'Mara', 'Noel', 'Orla', 'Pia']
FAMILIES = ['native-single', 'native-quorum', 'taproot-single', 'recovery-delay', 'recovery-height',
            'recovery-time', 'immediate-team', 'delayed-team', 'height-team', 'time-team',
            'joint-approval', 'joint-councils', 'alternative-recovery', 'joint-delay', 'joint-height', 'joint-time']


def write(path, value):
    with Path(path).open('x') as f:
        json.dump(value, f, indent=2); f.write('\n')


def main():
    rng = random.Random(202609071)
    cases = []
    for family in FAMILIES:
        for variant in range(8):
            names = rng.sample(NAMES, 6)
            order = list(range(6)); rng.shuffle(order)
            owner, backup, c, d, e, g = order
            key = lambda i: f'@{i}/**'
            pk = lambda i: f'pk(@{i})'
            time = 1730000000 + 86400 * (variant * 13 + FAMILIES.index(family))
            height = 840100 + variant * 257 + FAMILIES.index(family) * 11
            age = [6, 12, 48, 96, 192, 432, 864, 2016][variant]
            clock = ('older', age, f'the output has aged at least {age} blocks since confirmation')
            if 'height' in family:
                clock = ('after', height, f'the spending transaction has a block-height locktime of at least {height}')
            elif 'time' in family:
                clock = ('after', time, f'the spending transaction has a time-based locktime of at least {time} Unix seconds')
            op, value, phrase = clock
            lock = f'{op}({value})'
            k = variant % 3 + 1
            team = [backup, c, d]
            teamms = f'multi_a({k},' + ','.join(key(i) for i in team) + ')'
            teampol = f'thresh({k},' + ','.join(pk(i) for i in team) + ')'
            teamtext = f'at least {k} of ' + ', '.join(names[i] for i in team)
            route = policy = None
            intro = [f'Please configure a Taproot reserve for {names[owner]}.',
                     'I need a Taproot wallet for this arrangement.',
                     'Can you describe this account as a Taproot wallet?',
                     'Build a Taproot savings wallet.'][variant % 4]
            ownertext = f'{names[owner]} can always spend alone.'
            if family in ['native-single', 'taproot-single']:
                wrapper, account = ('wpkh', 'native SegWit P2WPKH') if family == 'native-single' else ('tr', 'Taproot')
                template = f'{wrapper}({key(owner)})'; policy = pk(owner)
                request = f'I want a {account} account for {names[owner]}. A signature from {names[owner]} is sufficient at any time. There are no other spending routes.'
            elif family == 'native-quorum':
                template = f'wsh(multi({k},' + ','.join(key(i) for i in team) + '))'
                policy = teampol
                request = f'Make a native SegWit P2WSH account. A withdrawal needs signatures from {teamtext}. There is no waiting period, and no other route.'
            elif family.startswith('recovery-'):
                route = f'and_v(v:pk({key(backup)}),{lock})'; policy = f'and({pk(backup)},{lock})'
                request = f'{intro} {ownertext} Independently, {names[backup]} can spend alone only once {phrase}. The owner remains unrestricted.'
            elif family in ['immediate-team', 'delayed-team', 'height-team', 'time-team']:
                route, policy = teamms, teampol
                request = f'{intro} {ownertext} A separate route requires signatures from {teamtext}, without {names[owner]}.'
                if family != 'immediate-team':
                    route, policy = f'and_v(v:{route},{lock})', f'and({policy},{lock})'
                    request += f' That team must also wait until {phrase}.'
            elif family in ['joint-approval', 'joint-delay', 'joint-height', 'joint-time']:
                route = f'and_v(v:pk({key(backup)}),pk({key(c)}))'; policy = f'and({pk(backup)},{pk(c)})'
                request = f'{intro} {ownertext} The second route needs both {names[backup]} and {names[c]}, without the owner.'
                if family != 'joint-approval':
                    route, policy = f'and_v(v:{route},{lock})', f'and({policy},{lock})'
                    request += f' This joint route is available only once {phrase}.'
            elif family == 'joint-councils':
                q = variant % 2 + 1
                secondms = f'multi_a({q},{key(e)},{key(g)})'; secondpol = f'thresh({q},{pk(e)},{pk(g)})'
                route, policy = f'and_v(v:{teamms},{secondms})', f'and({teampol},{secondpol})'
                request = f'{intro} {ownertext} The alternative needs {teamtext}, AND at least {q} of {names[e]} and {names[g]}. Each group must meet its own requirement; extra signatures in one group cannot replace approval from the other.'
            elif family == 'alternative-recovery':
                route = '{' + f'pk({key(e)}),and_v(v:{teamms},{lock})' + '}'
                policy = f'or({pk(e)},and({teampol},{lock}))'
                request = f'{intro} {ownertext} {names[e]} can also spend alone at any time. A third route requires {teamtext}, and is available only when {phrase}. The team does not need either immediate signer.'
            if route:
                template = f'tr({key(owner)},{route})'; policy = f'or({pk(owner)},{policy})'
                request += ' Permit exactly those routes.'
            # Remove unused names, then permute placeholders without changing their roles.
            used = [i for i in range(6) if f'@{i}' in policy]
            mapping = {old: new for new, old in enumerate(used)}
            import re
            remap = lambda s: re.sub(r'@(\d+)', lambda m: '@' + str(mapping[int(m[1])]), s)
            cases.append(dict(id=f'ws1-{family}-{variant:02}', family=family, group=f'ws1-{family}-{variant:02}',
                              request=request, keys=[names[i] for i in used], template=remap(template), policy=remap(policy)))
    assert len(cases) == 128
    write('training/wallet-sft-v1-catalog.json', dict(suite='wallet-sft-v1', evaluation_only=False, cases=cases))


if __name__ == '__main__':
    main()
