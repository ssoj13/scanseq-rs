import scanseq
from pprint import pp

S = scanseq.Scanner(roots=["d:/_demo", ], recursive=True, mask="*", min_len=5)
pp(S.result)
pp(S.result.seqs)
