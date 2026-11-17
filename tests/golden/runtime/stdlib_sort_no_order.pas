program StdlibSortNoOrder;

// Sort and BinarySearch without a comparer on an item type with no default order raise
// EInvalidOperation when called, even with no items or one (collections.md 10.2); with a
// comparer they work. The same for TArray.Sort / BinarySearch.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TFoo = class
    N: Integer;
    constructor Create(AN: Integer);
  end;

  TPoint = record
    X, Y: Integer;
  end;

constructor TFoo.Create(AN: Integer);
begin
  N := AN;
end;

function ByN(const A, B: TFoo): Integer;
begin
  Result := A.N - B.N;
end;

procedure Report(const E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

var
  L: TList<TFoo>;
  P: TList<TPoint>;
  A: TArray<TFoo>;
  Pt: TPoint;
  F: TFoo;
  Found: Integer;
begin
  L := TList<TFoo>.Create;
  try L.Sort; except on E: Exception do Report(E); end;
  try L.BinarySearch(nil, Found); except on E: Exception do Report(E); end;
  L.Add(TFoo.Create(2));
  try L.Sort; except on E: Exception do Report(E); end;
  L.Add(TFoo.Create(1));
  try L.Sort; except on E: Exception do Report(E); end;
  try L.Sort(nil); except on E: Exception do Report(E); end;
  L.Sort(TComparer<TFoo>.Construct(ByN));
  for F in L do
    Write(F.N, ' ');
  WriteLn;
  for F in L do
    F.Free;
  L.Free;

  P := TList<TPoint>.Create;
  try P.Sort; except on E: Exception do Report(E); end;
  Pt := Default(TPoint);
  try P.BinarySearch(Pt, Found); except on E: Exception do Report(E); end;
  P.Free;

  A := nil;
  try TArray.Sort<TFoo>(A); except on E: Exception do Report(E); end;
  try TArray.BinarySearch<TFoo>(A, nil, Found); except on E: Exception do Report(E); end;
  TArray.Sort<TFoo>(A, TComparer<TFoo>.Construct(ByN));
  WriteLn('with a comparer: ', Length(A));
end.
