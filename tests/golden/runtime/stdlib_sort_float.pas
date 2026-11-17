program StdlibSortFloat;

// Sorting Double and Single by the default order puts them in the total order of KeyCompare
// (collections.md 10.2): -INF, the negative numbers, 0.0 and -0.0 as equal (kept in their order
// by the stable sort), the positive numbers, INF, then the NaNs. BinarySearch finds a NaN.

uses System.SysUtils, System.Generics.Collections;

var
  L: TList<Double>;
  S: TArray<Single>;
  X: Double;
  Y: Single;
  Zero, Found: Integer;
begin
  Zero := 0;
  L := TList<Double>.Create;
  L.AddRange([3.5, 0.0 / Zero, -1.25, 1.0 / Zero, -0.0, 2.0, -1.0 / Zero, 0.0, 0.0 / Zero, -7.0]);
  L.Sort;
  for X in L do
    Write(FloatToStr(X), ' ');
  WriteLn;
  WriteLn('-0.0 then 0.0: ', FloatToStr(1.0 / L[3]), ' ', FloatToStr(1.0 / L[4]));
  WriteLn('NaN: ', L.BinarySearch(0.0 / Zero, Found), ' at ', Found);
  WriteLn('INF: ', L.BinarySearch(1.0 / Zero, Found), ' at ', Found);
  WriteLn('1.0: ', L.BinarySearch(1.0, Found), ' at ', Found);
  L.Free;

  S := [1.5, 0.0 / Zero, -2.5, -1.0 / Zero];
  TArray.Sort<Single>(S);
  for Y in S do
    Write(FloatToStr(Y), ' ');
  WriteLn;
end.
