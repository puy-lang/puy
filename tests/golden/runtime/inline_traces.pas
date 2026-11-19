program InlineTraces;

{$Q-}
// Calls of small routines are inlined into their callers (design/inlining.md), and nothing a
// program can observe changes: an exception raised in an inlined routine has the same stack
// trace (the routine's frame, its file and line, the call's line), the caller's `try` blocks
// catch it, a recursive routine keeps its calls, a routine
// compiled with `{$Q+}` keeps its overflow check in a `{$Q-}` caller, a method of a shared
// instantiation is named by its generic form, and a method called on `nil` fails at the call.

uses System.SysUtils, System.Generics.Collections;

type
  TBox = class
    Value: Integer;
    function Get: Integer;
    function Twice: Integer;
  end;

function TBox.Get: Integer;
begin
  Result := Value;
end;

function TBox.Twice: Integer;
begin
  Result := Get * 2;
end;

function At(const A: TArray<Integer>; I: Integer): Integer;
begin
  Result := A[I];
end;

function AtTwice(const A: TArray<Integer>; I: Integer): Integer;
begin
  Result := At(A, I) * 2;
end;

function Fact(N: Integer): Integer;
begin
  if N <= 1 then
    Result := 1
  else
    Result := N * Fact(N - 1);
end;

{$Q+}
function AddChecked(A, B: Integer): Integer;
begin
  Result := A + B;
end;
{$Q-}

procedure Show(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
  WriteLn(E.StackTrace);
end;

procedure Guarded(const A: TArray<Integer>);
begin
  try
    WriteLn(AtTwice(A, 5));
  finally
    WriteLn('finally ran');
  end;
end;

var
  A: TArray<Integer>;
  L: TList<Integer>;
  Boxes: TList<TBox>;
  B: TBox;
begin
  A := [1, 2, 3];
  WriteLn(At(A, 1), ' ', AtTwice(A, 2), ' ', Fact(5));
  try
    Guarded(A);
  except
    on E: Exception do
      Show(E);
  end;
  L := TList<Integer>.Create;
  L.Add(10);
  WriteLn(L[0]);
  try
    WriteLn(L[3]);
  except
    on E: Exception do
      Show(E);
  end;
  L.Free;
  try
    WriteLn(AddChecked(MaxInt, 1));
  except
    on E: Exception do
      Show(E);
  end;
  Boxes := TList<TBox>.Create;
  B := TBox.Create;
  B.Value := 21;
  Boxes.Add(B);
  WriteLn(Boxes[0].Twice);
  try
    WriteLn(Boxes[1].Value);
  except
    on E: Exception do
      Show(E);
  end;
  Boxes.Free;
  B.Free;
  B := nil;
  WriteLn(B.Twice);
end.
