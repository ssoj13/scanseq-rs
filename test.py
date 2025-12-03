import scanseq
from pprint import pp

S = scanseq.Scanner(roots=["d:/_demo", ], recursive=True, mask="*", min_len=5)
pp(S.result)
pp(S.result.seqs)


# Test from_file
path = 'D:/_demo/Srcs/Kz/kz.0000.tif'
S = scanseq.Scanner.from_file(path)
print(f"from_file({path}) = {S}")

# Compare with regular scan
result = scanseq.Scanner.get_seq('D:/_demo/Srcs/Kz', recursive=False)
print(f"get_seq found {len(result.seqs)} sequences:")
for seq in result.seqs:
    print(f"  {seq.pattern} [{seq.start}-{seq.end}]")


S2 = scanseq.Scanner(roots=["C:/Programs/Ntutil", ], recursive=True, mask="*", min_len=5)
pp(S2)

