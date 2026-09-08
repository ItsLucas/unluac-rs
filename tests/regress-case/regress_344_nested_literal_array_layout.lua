-- Reconstruct the complete mixed literal before promoting its SSA producers.
-- Both the nested allocation order and the nine-slot array layout must survive.
-- unluac: expect-contains [[tag = {]]
local row = { 1, 2, 3, 4, 5, 6, 7, 8, 9, tag = { 10, 11 } }
row[8] = nil
assert(#row == 9)
assert(row.tag[1] == 10 and row.tag[2] == 11)
print("regress_344", #row, row[7], row[9], row.tag[2])
