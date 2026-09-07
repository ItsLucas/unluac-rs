-- Nested forward exits must preserve the short-circuit condition and both continuations.
local function f(a,b,c)
local x=0
if a and (b or c) then
if not b then return x end
if (a and b) or c then
x=x+7
else
x=x+4
end
else
if b then
if a then
x=x+7
else
x=x+3
end
else
if not b then
x=x+6
else
x=x+5
end
end
end
return x
end
for i=0,7 do print(f(i%2==1,math.floor(i/2)%2==1,i>=4)) end
