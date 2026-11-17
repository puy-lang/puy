unit Utils;

// Generic routines and a generic method declared in the interface section (generics.md 1.2, 2.1):
// the body's heading writes the type parameters again, without the constraints. Each body of
// the overloads with the same number of type parameters completes the declaration with its
// parameters, whatever their order.

interface

type
  TUtil = class
    class function First<T>(const A: TArray<T>): T; static;
  end;

function Max<T>(const A, B: T; const Less: Boolean): T;
function Pick<T: class>(const A: T): T;
function Plus<T>(const A: T): T; overload;
function Plus<T>(const A, B: T): T; overload;

implementation

class function TUtil.First<T>(const A: TArray<T>): T;
begin
  Result := A[0];
end;

function Max<T>(const A, B: T; const Less: Boolean): T;
begin
  if Less then
    Result := B
  else
    Result := A;
end;

function Pick<T>(const A: T): T;
begin
  Result := A;
end;

function Plus<T>(const A, B: T): T;
begin
  Result := B;
end;

function Plus<T>(const A: T): T;
begin
  Result := A;
end;

end.
