program StdlibSortBadComparer;

// A comparer that raises stops the sort and the exception reaches the caller; a comparer that
// breaks its contract (the same sign both ways, or a sign that changes from call to call) is
// followed without an error. Either way the items are the items sorted, none lost or repeated
// (collections.md 10.3), for TList<T>.Sort and TArray.Sort.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

var
  Calls: Integer;

function RaiseAfter20(const A, B: string): Integer;
begin
  Inc(Calls);
  if Calls > 20 then
    raise EConvertError.Create('comparer gave up');
  Result := CompareStr(A, B);
end;

function AlwaysLess(const A, B: string): Integer;
begin
  Result := -1;
end;

function Flipping(const A, B: string): Integer;
begin
  Inc(Calls);
  Result := Calls mod 3 - 1;
end;

// The items in their default order, which is the same as the original ones when none is lost.
function Sorted(const Values: array of string): string;
var
  L: TList<string>;
  S: string;
begin
  L := TList<string>.Create(Values);
  L.Sort;
  Result := '';
  for S in L do
    Result := Result + S + ' ';
  L.Free;
end;

procedure Check(const Name: string; const L: TList<string>; const Original: string);
begin
  WriteLn(Name, ': count ', L.Count, ', same items: ', Sorted(L.ToArray) = Original);
end;

var
  L: TList<string>;
  A: TArray<string>;
  I: Integer;
  Original: string;
begin
  L := TList<string>.Create;
  for I := 0 to 39 do
    L.Add(IntToStr(I * 7 mod 40));
  Original := Sorted(L.ToArray);

  Calls := 0;
  try
    L.Sort(TComparer<string>.Construct(RaiseAfter20));
  except
    on E: EConvertError do
      WriteLn('raised: ', E.Message, ' after ', Calls, ' calls');
  end;
  Check('raising comparer', L, Original);

  L.Sort(TComparer<string>.Construct(AlwaysLess));
  Check('always less', L, Original);

  Calls := 0;
  L.Sort(TComparer<string>.Construct(Flipping));
  Check('flipping', L, Original);
  L.Free;

  A := ['d', 'b', 'a', 'c', 'e', 'g', 'f', 'i', 'h', 'j', 'k'];
  Original := Sorted(A);
  Calls := 0;
  try
    TArray.Sort<string>(A, TComparer<string>.Construct(RaiseAfter20));
  except
    on E: EConvertError do
      WriteLn('TArray raised: ', E.Message);
  end;
  WriteLn('TArray after the raise: count ', Length(A), ', same items: ', Sorted(A) = Original);
  Calls := 0;
  TArray.Sort<string>(A, TComparer<string>.Construct(Flipping));
  WriteLn('TArray flipping: count ', Length(A), ', same items: ', Sorted(A) = Original);
end.
