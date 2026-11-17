program StdlibComparerConstruct;

// Construct keeps the routine it is given, counted like any other value of a `reference to`
// type (collections.md 10.1): the variables an anonymous method captures live while the comparer
// does, and are let go of when the last reference to the comparer goes. No leak.

uses System.SysUtils, System.Generics.Defaults;

type
  TTracker = class(TRefObject)
    Calls: Integer;
    destructor Destroy; override;
  end;

destructor TTracker.Destroy;
begin
  WriteLn('tracker destroyed after ', Calls, ' calls');
  inherited;
end;

function MakeComparer: IComparer<Integer>;
var
  Tracker: TTracker;
begin
  Tracker := TTracker.Create;
  Result := TComparer<Integer>.Construct(
    function(const Left, Right: Integer): Integer
    begin
      Tracker.Calls := Tracker.Calls + 1;
      Result := KeyCompare(Left, Right);
    end);
end;

function MakeEqualityComparer: IEqualityComparer<Integer>;
var
  Tracker: TTracker;
begin
  Tracker := TTracker.Create;
  Result := TEqualityComparer<Integer>.Construct(
    function(const Left, Right: Integer): Boolean
    begin
      Tracker.Calls := Tracker.Calls + 1;
      Result := Left = Right;
    end,
    function(const Value: Integer): Int64
    begin
      Tracker.Calls := Tracker.Calls + 1;
      Result := Value;
    end);
end;

procedure Run;
var
  C, D: IComparer<Integer>;
  Cmp: TComparison<Integer>;
  E: IEqualityComparer<Integer>;
begin
  C := MakeComparer;
  WriteLn(C.Compare(1, 2), ' ', C.Compare(2, 1));
  D := C;
  C := nil;
  WriteLn('C cleared');
  WriteLn(D.Compare(3, 3));
  D := nil;
  WriteLn('D cleared');

  // The comparer and the variable both hold the routine
  Cmp := function(const Left, Right: Integer): Integer
    begin
      Result := Right - Left;
    end;
  C := TComparer<Integer>.Construct(Cmp);
  Cmp := nil;
  WriteLn(C.Compare(1, 2));
  C := nil;

  E := MakeEqualityComparer;
  WriteLn(E.Equals(5, 5), ' ', E.GetHashCode(7));
  E := nil;
  WriteLn('E cleared');
end;

begin
  Run;
  WriteLn('end');
end.
