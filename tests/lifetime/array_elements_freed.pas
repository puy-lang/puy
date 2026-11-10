// puy-test: memory-limit=1m
program ArrayElementsFreed;

uses System.SysUtils;

// Arrays of strings, records and arrays release their elements: when the last reference goes,
// when `SetLength` shrinks or copies, and when an element is replaced. The loop would run out of
// the 1 MB limit if anything were kept (lifetime.md 3.2, 3.11).

type
  TItem = record
    Name: string;
    Tags: array of string;
  end;

function Build(N: Integer): TArray<TItem>;
var
  I: Integer;
begin
  SetLength(Result, N);
  for I := 0 to N - 1 do
  begin
    Result[I].Name := 'item' + IntToStr(I);
    Result[I].Tags := ['a' + IntToStr(I), 'b'];
  end;
end;

var
  Items, Keep: TArray<TItem>;
  Grid: array of array of string;
  Round, Total: Integer;
begin
  Total := 0;
  for Round := 1 to 200 do
  begin
    Items := Build(50);
    Keep := Items;
    SetLength(Items, 10);
    Items[0].Tags[0] := 'changed';
    Keep := Copy(Items, 2, 3);
    SetLength(Grid, 20, 20);
    Grid[Round mod 20][0] := Items[1].Name;
    SetLength(Grid, 5);
    Total := Total + Length(Keep) + Length(Grid);
  end;
  WriteLn(Total, ' ', Items[0].Tags[0], ' ', Keep[0].Name);
end.
