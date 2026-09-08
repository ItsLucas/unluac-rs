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
    k16 = { nil, 116 }, k17 = { nil, 117 }, k18 = { nil, 118 },
    k19 = { nil, 119 }, k20 = { nil, 120 }, k21 = { nil, 121 },
    k22 = { nil, 122 }, k23 = { nil, 123 }, k24 = { nil, 124 },
    k25 = { nil, 125 }, k26 = { nil, 126 }, k27 = { nil, 127 },
    k28 = { nil, 128 }, k29 = { nil, 129 }, k30 = { nil, 130 },
    k31 = { nil, 131 }, k32 = { nil, 132 },
}
assert(t.k01 ~= t.k32 and t.k32[2] == 132)
local weak = setmetatable({}, { __mode = "v" })
weak[1] = t.k32
t.k32 = nil
collectgarbage("collect")
collectgarbage("collect")
assert(weak[1] == nil and #t == 50 and t.k01[2] == 101)
print("regress_371", weak[1] == nil, #t)
