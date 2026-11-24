program ByteArraysPuy;

// Dynamic arrays of `Byte`, held one byte per element (types.md 9; requirements §5.5), in what
// diff/dynarray/byte_arrays leaves out: `+`, Concat, Insert and Delete (which Delphi XE4 lacks),
// slices, `TArray<Byte>` in generic code (TList<Byte>, TDictionary<Byte, string>, a generic
// routine), the range errors of an element, and an element lent to a `var` parameter touched
// through another path (EAliasError), or dropped by SetLength during the call.

uses System.SysUtils, System.Generics.Collections;

type
  TByteArr = array of Byte;

const
  Whats: array[0..3] of string = ('read', 'write', 'copy', 'other');

procedure Show(const Title: string; const A: array of Byte);
var
  I: Integer;
begin
  Write(Title, ' (', Length(A), '):');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn;
end;

function Reversed<T>(const A: TArray<T>): TArray<T>;
var
  I: Integer;
begin
  SetLength(Result, Length(A));
  for I := 0 to High(A) do
    Result[High(A) - I] := A[I];
end;

var
  A: TByteArr;

procedure Touch(const What: string; var X: Byte);
begin
  try
    case What of
      'read': WriteLn(A[1]);
      'write': A[1] := 9;
      'copy': Show('copy', Copy(A));
      'other': A[0] := A[2] + 10;
    end;
    WriteLn(What, ': ok');
  except
    on E: EAliasError do
      WriteLn(What, ': ', E.Message);
  end;
  X := X + 1;
end;

procedure Drop(var X: Byte);
begin
  SetLength(A, 1);
  X := 99;
end;

var
  T: TArray<Byte>;
  L: TList<Byte>;
  D: TDictionary<Byte, string>;
  S: string;
  I: Integer;
begin
  A := [10, 20];
  T := [1, 120, 180];
  Show('concat', A + T);
  Show('Concat', Concat(T, [9, 8], T));
  Insert(77, T, 1);
  Show('insert', T);
  Insert([5, 6], T, 0);
  Show('insert arr', T);
  Delete(T, 2, 3);
  Show('delete', T);
  A := [10, 20, 30, 40, 50];
  Show('slice 1..3', A[1..3]);
  Show('slice ..1', A[..1]);
  Show('slice 3..', A[3..]);
  T := Reversed<Byte>(A);
  Show('reversed', T);
  L := TList<Byte>.Create;
  for I := 1 to 6 do
    L.Add((I * 77) mod 256);
  L.Insert(0, 255);
  L.Delete(3);
  Show('list', L.ToArray);
  WriteLn('IndexOf 52: ', L.IndexOf(52), ', IndexOf 98: ', L.IndexOf(98), ', Contains 129: ',
    L.Contains(129), ', Contains 1: ', L.Contains(1));
  L.Sort;
  Show('sorted', L.ToArray);
  L.Free;
  D := TDictionary<Byte, string>.Create;
  for I := 0 to 9 do
    D.Add(I * 25, IntToStr(I));
  if D.TryGetValue(225, S) then
    WriteLn('225 -> ', S);
  WriteLn('has 26: ', D.ContainsKey(26), ', count ', D.Count);
  D.Free;
  try
    WriteLn(A[5]);
  except
    on E: ERangeError do
      WriteLn('read: ', E.Message);
  end;
  try
    A[-1] := 1;
  except
    on E: ERangeError do
      WriteLn('write: ', E.Message);
  end;
  for S in Whats do
    Touch(S, A[1]);
  Show('after', A);
  try
    Drop(A[3]);
  except
    on E: ERangeError do
      WriteLn('dropped: ', E.Message);
  end;
  Show('after drop', A);
end.
