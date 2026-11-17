program StdlibTArrayRangeErrors;

// TArray.Sort with Index / Count outside the array raises ERangeError (collections.md 10.4) with
// the texts of the array ranges (bytecode.md 2.22), and leaves the array as it was. An empty
// range is allowed up to the end of the array.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

var
  A: TArray<Integer>;

procedure TrySort(Index, Count: Integer);
var
  X: Integer;
begin
  Write('Sort(', Index, ', ', Count, '): ');
  try
    TArray.Sort<Integer>(A, nil, Index, Count);
    for X in A do
      Write(X, ' ');
    WriteLn;
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end;

begin
  A := [4, 3, 2, 1];
  TrySort(0, -1);
  TrySort(-1, 2);
  TrySort(3, 2);
  TrySort(5, 0);
  TrySort(0, 5);
  TrySort(4, 0);
  TrySort(1, 3);
  A := nil;
  TrySort(0, 0);
  TrySort(0, 1);
end.
