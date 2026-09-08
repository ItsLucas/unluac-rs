-- The generic non-nil simplifier must not undo the transaction boundary either.
local t = {
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
}
t.tag = 67
for i = 1, 50 do t[i] = nil end
t[3] = 3
assert(#t == 0 and t.tag == 67)
print("regress_366", #t, t.tag, t[3])
