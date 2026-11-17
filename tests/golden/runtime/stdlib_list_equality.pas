program StdlibListEquality;

// TList<T> finds items by key equality (collections.md 5.2): routine values of TList<TProc>,
// NaN equal to NaN, records field by field, classes by identity unless Equals is overridden.
// The versions taking an IEqualityComparer<T> use its Equals instead; nil means key equality.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TPoint = record
    X, Y: Integer;
  end;

  TName = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

constructor TName.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

function TName.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TName) and (TName(Obj).Name = Name);
end;

function TName.GetHashCode: Int64;
begin
  Result := KeyHash(Name);
end;

procedure One;
begin
  WriteLn('one');
end;

procedure Two;
begin
  WriteLn('two');
end;

function Point(X, Y: Integer): TPoint;
begin
  Result := Default(TPoint);
  Result.X := X;
  Result.Y := Y;
end;

var
  Procs: TList<TProc>;
  Handler: TProc;
  Reals: TList<Double>;
  Points: TList<TPoint>;
  Names: TList<TName>;
  Objects: TList<TObject>;
  O: TObject;
  Strs: TList<string>;
  Comparer: IEqualityComparer<string>;
  Zero, NaN: Double;
begin
  WriteLn('TList<TProc>');
  Procs := TList<TProc>.Create;
  Procs.Add(One);
  Procs.Add(nil);
  Procs.Add(Two);
  Handler := Two;
  WriteLn(Procs.IndexOf(Two), ' ', Procs.IndexOf(Handler), ' ', Procs.IndexOf(nil), ' ',
    Procs.Contains(One));
  Handler := Procs[2];
  Handler();
  WriteLn(Procs.Remove(One), ' ', Procs.IndexOf(One), ' ', Procs.Count);
  Procs.Free;

  WriteLn('TList<Double>');
  Zero := 0.0;
  NaN := Zero / Zero;
  Reals := TList<Double>.Create([1.5, NaN, 0.0]);
  WriteLn(Reals.IndexOf(NaN), ' ', Reals.IndexOf(-0.0), ' ', Reals.IndexOf(1.5));
  Reals.Free;

  WriteLn('TList<TPoint>');
  Points := TList<TPoint>.Create([Point(1, 2), Point(3, 4), Point(1, 2)]);
  WriteLn(Points.IndexOf(Point(1, 2)), ' ', Points.LastIndexOf(Point(1, 2)), ' ',
    Points.IndexOf(Point(2, 1)));
  Points.Free;

  WriteLn('classes');
  Names := TList<TName>.Create([TName.Create('a'), TName.Create('b')]);
  WriteLn(Names.IndexOf(TName.Create('b')), ' ', Names.Contains(TName.Create('c')));
  Names.Free;
  Objects := TList<TObject>.Create;
  O := TObject.Create;
  Objects.Add(TObject.Create);
  Objects.Add(O);
  WriteLn(Objects.IndexOf(O), ' ', Objects.Remove(O), ' ', Objects.Contains(O));
  Objects[0].Free;
  O.Free;
  Objects.Free;

  WriteLn('IEqualityComparer<string>');
  Strs := TList<string>.Create(['Apple', 'banana', 'APPLE']);
  Comparer := TStringComparer.OrdinalIgnoreCase;
  WriteLn(Strs.IndexOf('apple'), ' ', Strs.IndexOf('apple', Comparer), ' ',
    Strs.LastIndexOf('apple', Comparer), ' ', Strs.Contains('BANANA', Comparer), ' ',
    Strs.Contains('BANANA'));
  WriteLn(Strs.Remove('APPLE', Comparer), ' ', Strs.Count, ' ', Strs[0]);
  Comparer := TEqualityComparer<string>.Construct(
    function(const Left, Right: string): Boolean
    begin
      WriteLn('  compare ', Left, ' ', Right);
      Result := Length(Left) = Length(Right);
    end,
    function(const Value: string): Int64
    begin
      Result := Length(Value);
    end);
  WriteLn(Strs.IndexOf('xxxxx', Comparer));
  Comparer := nil;
  WriteLn(Strs.IndexOf('banana', Comparer), ' ', Strs.IndexOf('BANANA', Comparer), ' ',
    Strs.LastIndexOf('APPLE', Comparer), ' ', Strs.Contains('APPLE', Comparer), ' ',
    Strs.Remove('banana', Comparer));
  Strs.Free;
end.
