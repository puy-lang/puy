program PublishedOverrideAccess;

// Check that public and published methods override in both directions without E0744,
// and that calls through the base type dispatch to the overrides.

type
  TBase = class
  public
    function FromPublic: Integer; virtual;
  published
    function FromPublished: Integer; virtual;
  end;

  TChild = class(TBase)
  public
    function FromPublished: Integer; override;
  published
    function FromPublic: Integer; override;
  end;

function TBase.FromPublic: Integer;
begin
  Result := 1;
end;

function TBase.FromPublished: Integer;
begin
  Result := 2;
end;

function TChild.FromPublic: Integer;
begin
  Result := 3;
end;

function TChild.FromPublished: Integer;
begin
  Result := 4;
end;

var
  Sample: TBase;
begin
  Sample := TChild.Create;
  WriteLn(Sample.FromPublic, ' ', Sample.FromPublished);
  Sample.Free;
end.
