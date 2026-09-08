-- A pure explicit suffix may join only when allocation and physical replay still match.
local t = {
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
    k01 = { nil, 101 }, k02 = { nil, 102 }, k03 = { nil, 103 },
    k04 = { nil, 104 }, k05 = { nil, 105 }, k06 = { nil, 106 },
    k07 = { nil, 107 }, k08 = { nil, 108 }, k09 = { nil, 109 },
    k10 = { nil, 110 }, k11 = { nil, 111 }, k12 = { nil, 112 },
    k13 = { nil, 113 }, k14 = { nil, 114 }, k15 = { nil, 115 },
    k16 = { nil, 116 }, k17 = { nil, 117 },
}
t.k18 = { nil, 118 }
assert(t.k01 ~= t.k18 and t.k18[2] == 118)
local weak = setmetatable({}, { __mode = "v" })
weak[1] = t.k18
t.k18 = nil
collectgarbage("collect")
collectgarbage("collect")
assert(weak[1] == nil and #t == 50 and t.k01[2] == 101)
print("regress_374", weak[1] == nil, #t)
