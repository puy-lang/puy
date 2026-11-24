program LegacyObjects;

uses
  System.Contnrs;

type
  TJob = class
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

constructor TJob.Create(const AName: string);
begin
  inherited Create;
  Name := AName;
end;

destructor TJob.Destroy;
begin
  WriteLn('Releasing ', Name);
  inherited Destroy;
end;

begin
  // Use the non-generic list when working with an API that accepts TObject.
  var Jobs := TObjectList.Create(True);
  try
    Jobs.Add(TJob.Create('print'));
    Jobs.Add(TJob.Create('archive'));
    var NextJob := Jobs.Extract(Jobs[0]) as TJob;
    try
      WriteLn('Running ', NextJob.Name);
    finally
      NextJob.Free; // Extract transfers ownership to the caller.
    end;
  finally
    Jobs.Free; // Frees the job still in the list.
  end;
end.
