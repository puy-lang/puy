program ProcValueMoreErrors;

// More errors with procedural values (objects.md 9.2, 9.5, 9.10): members of `TObject` that are
// not virtual cannot be values (E0752: `ClassType`, `InheritsFrom`); a `const` parameter does not
// match a value parameter, nor an open array of another element type, nor an overload set with no
// matching signature (E0702); `@` and `@@` are not allowed (E3004, one for each `@`); a
// procedural value cannot be written (E0718); `reference` alone is not a type (E0700).

type
  TEvent = procedure of object;
  TConstFn = function(const X: Integer): Integer;
  TSumInts = function(const A: array of Integer): Integer;
  TStrFn = function(S: string): string;
  TPlainFn = function(X: Integer): Integer;

function Id(X: Integer): Integer;
begin
  Result := X;
end;

function SumReals(const A: array of Double): Integer;
begin
  Result := Length(A);
end;

function Ov(X: Integer): Integer; overload;
begin
  Result := X;
end;

function Ov(X: Double): Integer; overload;
begin
  Result := 0;
end;

var
  O: TObject;
  E: TEvent;
  C: TConstFn;
  S: TSumInts;
  F: TStrFn;
  Fn: TPlainFn;
  R: TProc;
  X: reference;
begin
  O := TObject.Create;
  E := O.ClassType;
  E := O.InheritsFrom;
  C := Id;
  S := SumReals;
  F := Ov;
  Fn := @Id;
  Fn := @@Fn;
  R := nil;
  WriteLn(R);
  O.Free;
end.
