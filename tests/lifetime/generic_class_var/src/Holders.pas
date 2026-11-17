unit Holders;

interface

type
  TTag = class(TRefObject)
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  // Each instantiation has its own `class var`s (generics.md 2.5)
  THolder<T> = class
  public
    class var Tag: TTag;
    class var Values: TArray<T>;
    class procedure Keep(const AName: string; const X: T);
  end;

implementation

constructor TTag.Create(const AName: string);
begin
  Name := AName;
end;

destructor TTag.Destroy;
begin
  WriteLn('free ', Name);
  inherited;
end;

class procedure THolder<T>.Keep(const AName: string; const X: T);
begin
  Tag := TTag.Create(AName);
  SetLength(Values, Length(Values) + 1);
  Values[High(Values)] := X;
end;

finalization
  WriteLn('fini Holders');
end.
