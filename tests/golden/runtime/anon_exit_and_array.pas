program AnonExitAndArray;

// An anonymous method can be written in every value position (objects.md 9.4): as the value of
// `Exit(…)` and as an element of an array constructor next to a function result.

type
  TGreet = reference to function(const S: string): string;

function Mk(const Prefix: string): TGreet;
begin
  Exit(function(const S: string): string
    begin
      Result := Prefix + S;
    end);
end;

var
  Items: TArray<TGreet>;
begin
  Items := [function(const S: string): string begin Result := 'anon ' + S end, Mk('mk ')];
  WriteLn(Length(Items));
  for var G in Items do
    WriteLn(G('x'));
end.
