program StdlibComparerLifetime;

// A comparer is a reference-counted object held through its interface (collections.md 10.1): it
// is destroyed when the last interface reference goes, never by Free. Two variables can share one
// comparer. (Inside a routine, so that no temporary of the main block holds a comparer.)

uses System.SysUtils, System.Generics.Defaults;

type
  TTraced = class(TComparer<Integer>)
  public
    function Compare(const Left, Right: Integer): Integer; override;
    destructor Destroy; override;
  end;

  TTracedEquality = class(TEqualityComparer<string>)
  public
    function Equals(const Left, Right: string): Boolean; override;
    function GetHashCode(const Value: string): Int64; override;
    destructor Destroy; override;
  end;

function TTraced.Compare(const Left, Right: Integer): Integer;
begin
  Result := KeyCompare(Left, Right);
end;

destructor TTraced.Destroy;
begin
  WriteLn('comparer destroyed');
  inherited;
end;

function TTracedEquality.Equals(const Left, Right: string): Boolean;
begin
  Result := Left = Right;
end;

function TTracedEquality.GetHashCode(const Value: string): Int64;
begin
  Result := Length(Value);
end;

destructor TTracedEquality.Destroy;
begin
  WriteLn('equality comparer destroyed');
  inherited;
end;

procedure Use(const C: IComparer<Integer>);
begin
  WriteLn('compare ', C.Compare(1, 1));
end;

procedure Run;
var
  A, B: IComparer<Integer>;
  E: IEqualityComparer<string>;
begin
  A := TTraced.Create;
  B := A;
  Use(A);
  A := nil;
  WriteLn('A cleared');
  Use(B);
  B := TComparer<Integer>.Default;
  WriteLn('B replaced');
  E := TTracedEquality.Create;
  WriteLn(E.Equals('x', 'x'), ' ', E.GetHashCode('abc'));
  E := nil;
  WriteLn('run end');
end;

begin
  Run;
  WriteLn('end');
end.
