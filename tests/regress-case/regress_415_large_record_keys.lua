-- Crossing RK index 255 changes keys and numeric values to stack operands.
-- Constants are retained in real ignored calls to preserve the compiler frame.
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
local reads = 0
local leaf = setmetatable({}, {
    __index = function(_, key)
        assert(Record398Rows == "old")
        reads = reads + 1
        if Record415Fail == reads then error("lookup-stop", 0) end
        trace[#trace + 1] = "field:" .. key .. ":" .. reads
        if key == "missing" then return nil end
        return reads
    end
})
Record398Catalog = setmetatable({}, {
    __index = function(_, key)
        trace[#trace + 1] = "group:" .. key
        return leaf
    end
})
function Record398Load(name)
    trace[#trace + 1] = "load:" .. name
    return "discarded", {}
end
function Record398Build()
    Record398Load(3)
    Record398Load(
        "pool415_0", "pool415_1", "pool415_2", "pool415_3", "pool415_4", "pool415_5", "pool415_6", "pool415_7",
        "pool415_8", "pool415_9", "pool415_10", "pool415_11", "pool415_12", "pool415_13", "pool415_14", "pool415_15",
        "pool415_16", "pool415_17", "pool415_18", "pool415_19", "pool415_20", "pool415_21", "pool415_22", "pool415_23",
        "pool415_24", "pool415_25", "pool415_26", "pool415_27", "pool415_28", "pool415_29", "pool415_30", "pool415_31",
        "pool415_32", "pool415_33", "pool415_34", "pool415_35", "pool415_36", "pool415_37", "pool415_38", "pool415_39",
        "pool415_40", "pool415_41", "pool415_42", "pool415_43", "pool415_44", "pool415_45", "pool415_46", "pool415_47",
        "pool415_48", "pool415_49", "pool415_50", "pool415_51", "pool415_52", "pool415_53", "pool415_54", "pool415_55",
        "pool415_56", "pool415_57", "pool415_58", "pool415_59", "pool415_60", "pool415_61", "pool415_62", "pool415_63",
        "pool415_64", "pool415_65", "pool415_66", "pool415_67", "pool415_68", "pool415_69", "pool415_70", "pool415_71",
        "pool415_72", "pool415_73", "pool415_74", "pool415_75", "pool415_76", "pool415_77", "pool415_78", "pool415_79",
        "pool415_80", "pool415_81", "pool415_82", "pool415_83", "pool415_84", "pool415_85", "pool415_86", "pool415_87",
        "pool415_88", "pool415_89", "pool415_90", "pool415_91", "pool415_92", "pool415_93", "pool415_94", "pool415_95",
        "pool415_96", "pool415_97", "pool415_98", "pool415_99", "pool415_100", "pool415_101", "pool415_102", "pool415_103",
        "pool415_104", "pool415_105", "pool415_106", "pool415_107", "pool415_108", "pool415_109", "pool415_110", "pool415_111",
        "pool415_112", "pool415_113", "pool415_114", "pool415_115", "pool415_116", "pool415_117", "pool415_118", "pool415_119",
        "pool415_120", "pool415_121", "pool415_122", "pool415_123")
    Record398Load(
        "pool415_124", "pool415_125", "pool415_126", "pool415_127", "pool415_128", "pool415_129", "pool415_130", "pool415_131",
        "pool415_132", "pool415_133", "pool415_134", "pool415_135", "pool415_136", "pool415_137", "pool415_138", "pool415_139",
        "pool415_140", "pool415_141", "pool415_142", "pool415_143", "pool415_144", "pool415_145", "pool415_146", "pool415_147",
        "pool415_148", "pool415_149", "pool415_150", "pool415_151", "pool415_152", "pool415_153", "pool415_154", "pool415_155",
        "pool415_156", "pool415_157", "pool415_158", "pool415_159", "pool415_160", "pool415_161", "pool415_162", "pool415_163",
        "pool415_164", "pool415_165", "pool415_166", "pool415_167", "pool415_168", "pool415_169", "pool415_170", "pool415_171",
        "pool415_172", "pool415_173", "pool415_174", "pool415_175", "pool415_176", "pool415_177", "pool415_178", "pool415_179",
        "pool415_180", "pool415_181", "pool415_182", "pool415_183", "pool415_184", "pool415_185", "pool415_186", "pool415_187",
        "pool415_188", "pool415_189", "pool415_190", "pool415_191", "pool415_192", "pool415_193", "pool415_194", "pool415_195",
        "pool415_196", "pool415_197", "pool415_198", "pool415_199", "pool415_200", "pool415_201", "pool415_202", "pool415_203",
        "pool415_204", "pool415_205", "pool415_206", "pool415_207", "pool415_208", "pool415_209", "pool415_210", "pool415_211",
        "pool415_212", "pool415_213", "pool415_214", "pool415_215", "pool415_216", "pool415_217", "pool415_218", "pool415_219",
        "pool415_220", "pool415_221", "pool415_222", "pool415_223", "pool415_224", "pool415_225", "pool415_226", "pool415_227",
        "pool415_228", "pool415_229", "pool415_230", "pool415_231", "pool415_232", "pool415_233", "pool415_234", "pool415_235",
        "pool415_236", "pool415_237", "pool415_238", "pool415_239", "pool415_240", "pool415_241", "pool415_242", "pool415_243",
        "pool415_244", "pool415_245", "pool415_246", "pool415_247")

    Record398Rows = {
        -- region398 rows begin
        {alpha = Record398Catalog.group.alpha, missing = Record398Catalog.group.missing, pad = 3},
        {alpha = Record398Catalog.group.alpha, missing = Record398Catalog.group.missing, pad = 3},
        -- region398 rows end
    }
end
Record398Rows = "old"
Record398Build()
assert(type(Record398Rows[1]) == "table" and Record398Rows[1].missing == nil)
print(#Record398Rows, reads, table.concat(trace, "|"))

for fail = 1, 4 do
    reads = 0
    Record415Fail = fail
    Record398Rows = "old"
    local ok, err = pcall(Record398Build)
    assert(not ok and err == "lookup-stop" and Record398Rows == "old")
    print(fail, reads)
end
