-- 全局构造后多读的固定调用声明必须作为源码帧锚，后续循环不应撤销已证明的构造。
-- unluac: expect-not-contains [[unluac error]]
local trace = {}
function Anchor429Catalog(input) return input end
function Anchor429Make(value)
    collectgarbage('collect')
    trace[#trace + 1] = 'make:' .. value
    return {value = value}
end
function Anchor429Object() return {n = 2, b = 7} end
function Anchor429Log(value) trace[#trace + 1] = value end
function Anchor429Build(input)
    local catalog = Anchor429Catalog(input)
    Anchor429Data = {
        {Anchor429Make(1), 'a:' .. catalog.name},
        {Anchor429Make(2), 'b:' .. catalog.name},
    }
    local object = Anchor429Object()
    repeat
        Anchor429Log(object.n)
        object.n = object.n - 1
    until object.n == 0
    Anchor429Log(object.b)
end
Anchor429Build({name = 'item'})
assert(Anchor429Data[1][1].value == 1 and Anchor429Data[2][2] == 'b:item')
print(table.concat(trace, '|'))
